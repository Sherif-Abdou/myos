use core::arch::{asm, naked_asm};

use crate::{
    allocators::KERNEL_ALLOCATOR,
    cpu_local,
    interrupts::{ExceptionRegisters, daifclr, daifset},
    printk,
    sched::Task,
    timer::ms_sleep,
    utils::{Arc, CpuLocal, List, ListArc, OnceSpinLock, SpinLock, with_core_critical_section},
};

pub static SCHEDULER: OnceSpinLock<Sched> = OnceSpinLock::new();

pub const STACK_VIRTUAL_ADDR: usize = 0x800000;

pub fn cpu_current_task() -> Option<Arc<Task>> {
    SCHEDULER.get().unwrap().local_task()
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
    SCHEDULER
        .get()
        .unwrap()
        .task_from_fn(kill_thread, core::ptr::null_mut());
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
pub(crate) extern "C" fn restore_regs_and_eret(regs: *const ExceptionRegisters) {
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

fn kill_thread(_arg: *mut ()) {
    loop {
        SCHEDULER.get().unwrap().flush_kill_queue();
        ms_sleep(1000);
    }
}

impl Sched {
    pub fn local_task(&self) -> Option<Arc<Task>> {
        self.scheduled.local().lock().as_ref().cloned()
    }

    pub fn append_task(&self, task: impl Into<ListArc<Task, 0>>) {
        let mut run_queue = self.run_queue.lock();

        run_queue.push_back(task.into());
    }

    pub fn block_this_task(&self) {
        with_core_critical_section(|| {
            let task = self.staging.local().lock().take().unwrap();

            task.mark_blocked();

            // let task = unsafe { self.run_queue.lock().remove_at(&task) };

            self.blocked_queue.lock().push_back(task);
        });
    }

    pub fn unblock_task(&self, task: &Arc<Task>) {
        let mut run_queue = self.run_queue.lock();
        let task = unsafe { self.blocked_queue.lock().remove_at_unchecked(task) };

        task.mark_runnable();

        run_queue.push_back(task);
    }

    pub fn end_task(&self, code: i32) {
        let mut kill_queue = self.kill_queue.lock();
        let mut current = self.staging.local().lock();

        if let Some(task) = current.take() {
            task.mark_done(code);
            if !task.has_parent() {
                kill_queue.push_back(task);
            }
        }
    }

    #[allow(clippy::fn_to_numeric_cast, function_casts_as_integer)]
    pub fn task_from_fn(&self, f: fn(*mut ()), arg: *mut ()) {
        let task = Task::kernel_task_from_fn(f, arg);

        self.run_queue.lock().push_back(task.into());
    }

    pub fn load_program(&self, elf: &[u8]) {
        let task = Task::load_program(elf);

        self.run_queue.lock().push_back(task.into());
    }

    #[allow(clippy::fn_to_numeric_cast, function_casts_as_integer)]
    pub fn idle_task_from_fn(&self, f: fn(*mut ()), arg: *mut ()) {
        let task = Task::kernel_task_from_fn(f, arg);

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
            if let Some(ref scheduled) = *scheduled
                && !scheduled.is_done()
            {
                scheduled.mark_runnable();
            }

            task.bind_pages();

            task.mark_running();

            *scheduled = Some(task.clone_arc());

            *staging = Some(task);

            let registers = scheduled.as_ref().unwrap().registers.lock();

            Some(registers.clone())
        } else {
            // Run the idle task.
            let idle = self.idle_task.local().lock();
            let mut scheduled = self.scheduled.local().lock();

            idle.as_ref().unwrap().mark_running();
            if let Some(ref scheduled) = *scheduled {
                scheduled.mark_runnable();
            }

            *scheduled = Some(idle.as_ref().unwrap().clone_arc());

            Some(scheduled.as_ref().unwrap().registers.lock().clone())
        }
    }

    pub fn flush_kill_queue(&self) {
        let mut kill_queue = self.kill_queue.lock();
        let mut cursor = kill_queue.cursor_mut();

        while let Some(task) = cursor.get() {
            task.kill_children();
            cursor.remove();
        }

        drop(kill_queue);
    }
}
