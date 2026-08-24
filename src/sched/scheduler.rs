use core::arch::{asm, naked_asm};

use crate::{
    allocators::kvec, cpu_local, elf::ElfParser, interrupts::{ExceptionRegisters, daifclr, daifset}, printk, sched::{
        STACK_SIZE, Task, TaskFdTable, TaskInfo, UserSpaceTaskInfo, create_kernel_stack,
        create_user_stack,
    }, subsystem::ArmPageTableRoot, utils::{
        Arc, CpuLocal, List, ListArc, ListLinks, OnceSpinLock, SpinLock, UniqueArc, cpu_id, with_core_critical_section,
    },
};

pub static SCHEDULER: OnceSpinLock<Sched> = OnceSpinLock::new();

extern "C" fn thread_wrapper(f: extern "C" fn(*mut ()), arg: *mut ()) {
    f(arg);

    SCHEDULER.get().unwrap().end_task();

    let next_task = SCHEDULER.get().unwrap().next_task().unwrap();

    with_core_critical_section(|| {
        restore_regs_and_eret(&raw const next_task);
    });
}

pub fn init_scheduler() {
    assert!(
        SCHEDULER
            .set(Sched {
                run_queue: SpinLock::new(List::new()),
                blocked_queue: SpinLock::new(List::new()),
                kill_queue: SpinLock::new(List::new()),
                scheduled: cpu_local!(SpinLock::new(None)),
                staging: cpu_local!(SpinLock::new(None)),
                idle_task: cpu_local!(SpinLock::new(None)),
            })
            .is_ok(),
        "Couldn't initialize scheduler"
    );
    create_local_idle_task();
}

pub fn create_local_idle_task() {
    SCHEDULER
        .get()
        .unwrap()
        .idle_task_from_fn(threaded_idle, core::ptr::null_mut());
}

pub struct Sched {
    // Tasks that are available to run.
    run_queue: SpinLock<List<Task>>,
    // Tasks that are blocked.
    blocked_queue: SpinLock<List<Task>>,
    // Tasks to be killed on the next tick.
    kill_queue: SpinLock<List<Task>>,
    // Idle task
    idle_task: CpuLocal<SpinLock<Option<ListArc<Task, 0>>>>,
    staging: CpuLocal<SpinLock<Option<ListArc<Task, 0>>>>,
    scheduled: CpuLocal<SpinLock<Option<Arc<Task>>>>,
}

#[unsafe(naked)]
extern "C" fn save_callee_regs(dst: *mut u64, new_pc: usize) {
    naked_asm!(
        "stp x18, x19, [x0]",
        "stp x20, x21, [x0, #0x10]",
        "stp x22, x23, [x0, #0x20]",
        "stp x24, x25, [x0, #0x30]",
        "stp x26, x27, [x0, #0x40]",
        "stp x28, x29, [x0, #0x50]",
        "mov x29, sp",
        "stp x30, x29, [x0, #0x60]",
        "mrs x2, nzcv",
        "mrs x3, daif",
        "orr x2, x2, x3",
        "mov x3, #0b0101",
        "orr x2, x2, x3",
        "stp x1, x2, [x0, #0x70]",
        "ret"
    )
}

#[unsafe(naked)]
extern "C" fn restore_regs_and_eret(regs: *const ExceptionRegisters) {
    naked_asm!(
        r#"
    ldp x8, x9, [x0, 0x110]
    msr sp_el0, x8
    ldp x8, x9, [x0, 0x100]
    msr elr_el1, x8
    msr spsr_el1, x9
    ldp x30, x29, [x0, 0xf0]
    mov sp, x29
    ldp x28, x29, [x0, 0xe0]
    ldp x26, x27, [x0, 0xd0]
    ldp x24, x25, [x0, 0xc0]
    ldp x22, x23, [x0, 0xb0]
    ldp x20, x21, [x0, 0xa0]
    ldp x18, x19, [x0, 0x90]
    ldp x16, x17, [x0, 0x80]
    ldp x14, x15, [x0, 0x70]
    ldp x12, x13, [x0, 0x60]
    ldp x10, x11, [x0, 0x50]
    ldp x8, x9, [x0, 0x40]
    ldp x6, x7, [x0, 0x30]
    ldp x4, x5, [x0, 0x20]
    ldp x2, x3, [x0, 0x10]
    ldp x0, x1, [x0]
    eret
        "#
    )
}

pub fn sched_yield() {
    {
        daifset();

        let mut addr: usize = 0;
        unsafe {
            asm!("ldr {0}, =1f", out(reg) addr);
        }

        let this_task = SCHEDULER.get().unwrap().local_task().unwrap();

        let mut regs = this_task.registers.lock();
        let reg_ptr = unsafe { regs.gprs.as_mut_ptr().add(18) };
        save_callee_regs(reg_ptr, addr);
        drop(regs);
    }

    let next_task = SCHEDULER.get().unwrap().next_task().unwrap();

    with_core_critical_section(|| {
        restore_regs_and_eret(&raw const next_task);
    });

    unsafe {
        asm!("1: nop");
    }

    daifclr();
}

fn threaded_idle(_arg: *mut ()) {
    daifclr();

    loop {
        unsafe {
            asm!("wfi");
        }
    }
}

impl Sched {
    pub fn local_task(&self) -> Option<Arc<Task>> {
        self.scheduled.local().lock().as_ref().cloned()
    }

    pub fn block_this_task(&self) {
        with_core_critical_section(|| {
            let task = self.staging.local().lock().take().unwrap();

            // let task = unsafe { self.run_queue.lock().remove_at(&task) };

            self.blocked_queue.lock().push_back(task);
        });
    }

    pub fn unblock_task(&self, task: &Arc<Task>) {
        let task = unsafe { self.blocked_queue.lock().remove_at(task) };

        self.run_queue.lock().push_back(task);
    }

    pub fn end_task(&self) {
        let mut kill_queue = self.kill_queue.lock();
        let mut current = self.staging.local().lock();

        if let Some(task) = current.take() {
            kill_queue.push_back(task);
        }
    }

    #[allow(clippy::fn_to_numeric_cast, function_casts_as_integer)]
    pub fn task_from_fn(&self, f: fn(*mut ()), arg: *mut ()) {
        let task = UniqueArc::new(Task {
            registers: SpinLock::new(ExceptionRegisters::default()),
            links: ListLinks::new(),
            task_info: Arc::new(TaskInfo::new_kernel(create_kernel_stack())),
        });

        let mut registers = task.registers.lock();
        registers.elr = (thread_wrapper) as u64;
        registers.gprs[0] = (f as usize) as u64;
        registers.gprs[1] = arg as u64;
        registers.gprs[31] = task.stack_top();
        assert!(
            registers.gprs[31].is_multiple_of(16),
            "Stack ptr is not aligned"
        );
        registers.spsr = 0b0101;
        drop(registers);

        self.run_queue.lock().push_back(task.into());
    }

    pub fn load_program(&self, elf: &[u8]) {
        const STACK_VIRTUAL_ADDR: usize = 0x800000;

        let parser = ElfParser::new(elf);

        let num_segments = parser.num_segments();

        let mut segments = kvec();
        let page_table = ArmPageTableRoot::create_user();

        let user_stack = create_user_stack();

        for index in 0..num_segments {
            let segment = parser.segment(index);
            if !segment.is_null() {
                let phys_addr = segment.loaded_phys_addr();
                let virt_addr = segment.virt_addr() & !0xfff;
                let pages = segment.mem_page_count();

                page_table.map_page_range(virt_addr, phys_addr, pages);

                segments.push(segment);
            }
        }

        page_table.map_page_range(
            STACK_VIRTUAL_ADDR,
            user_stack.phys_addr(),
            user_stack.len().div_ceil(4096),
        );

        let userspace = UserSpaceTaskInfo {
            page_table: SpinLock::new(page_table),
            segments,
            user_stack,
            kernel_stack: create_kernel_stack(),
            fds: TaskFdTable::new(),
        };

        let task = UniqueArc::new(Task {
            registers: SpinLock::new(ExceptionRegisters::default()),
            links: ListLinks::new(),
            task_info: Arc::new(TaskInfo::User(userspace)),
        });

        let mut registers = task.registers.lock();
        registers.elr = parser.entry_vma() as u64;
        registers.gprs[31] = task.kernel_stack_top();
        registers.sp_el0 = STACK_VIRTUAL_ADDR as u64 + STACK_SIZE as u64;

        assert!(
            registers.gprs[31].is_multiple_of(16),
            "Stack ptr is not aligned"
        );
        registers.spsr = 0b0000;
        drop(registers);

        self.run_queue.lock().push_back(task.into());
    }

    #[allow(clippy::fn_to_numeric_cast, function_casts_as_integer)]
    pub fn idle_task_from_fn(&self, f: fn(*mut ()), arg: *mut ()) {
        let task = UniqueArc::new(Task {
            registers: SpinLock::new(ExceptionRegisters::default()),
            links: ListLinks::new(),
            task_info: Arc::new(TaskInfo::new_kernel(create_kernel_stack())),
        });

        let mut registers = task.registers.lock();
        registers.elr = (thread_wrapper) as u64;
        registers.gprs[0] = (f as usize) as u64;
        registers.gprs[1] = arg as u64;
        registers.gprs[31] = task.stack_top();
        assert!(
            registers.gprs[31].is_multiple_of(16),
            "Stack ptr is not aligned"
        );
        registers.spsr = 0b0101;
        drop(registers);

        *self.idle_task.local().lock() = Some(task.into());
    }

    pub fn save_register_state_to_task(&self, state: &ExceptionRegisters) {
        let Some(task) = self.local_task() else {
            return;
        };
        let mut registers = task.registers.lock();
        *registers = state.clone();
    }

    pub fn next_task(&self) -> Option<ExceptionRegisters> {
        let mut tasks = self.run_queue.lock();

        let mut staging = self.staging.local().lock();

        if let Some(staging) = staging.take() {
            tasks.push_back(staging);
        }

        let task = tasks.remove_front();

        if let Some(task) = task {
            let mut scheduled = self.scheduled.local().lock();

            if let TaskInfo::User(ref userspace) = *task.task_info {
                userspace.page_table.lock().bind_user();
            }

            *scheduled = Some(task.clone_arc());

            *staging = Some(task);
            //
            // printk!("Scheduled a real task on core: {}\n", cpu_id());

            Some(scheduled.as_ref().unwrap().registers.lock().clone())
        } else {
            // Run the idle task.
            let mut scheduled = self.scheduled.local().lock();
            let idle = self.idle_task.local().lock();
            *scheduled = Some(idle.as_ref().unwrap().clone_arc());

            Some(scheduled.as_ref().unwrap().registers.lock().clone())
        }
    }

    pub fn flush_kill_queue(&self) {
        let mut kill_queue = self.kill_queue.lock();

        while !kill_queue.is_empty() {
            kill_queue.remove_front();
        }

        drop(kill_queue);
    }
}
