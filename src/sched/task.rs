use core::arch::{asm, naked_asm};

use crate::{
    allocators::{KBox, KERNEL_ALLOCATOR},
    impl_link,
    interrupts::{ExceptionRegisters, daifclr, daifset},
    utils::{Arc, List, ListLinks, OnceSpinLock, SpinLock, UniqueArc},
};

const STACK_SIZE: usize = 4096 * 16;

#[repr(align(16))]
struct TaskStack([u8; STACK_SIZE]);

#[repr(align(16))]
pub struct Task {
    registers: SpinLock<ExceptionRegisters>,
    links: ListLinks,
    stack: KBox<TaskStack>,
}

pub static SCHEDULER: OnceSpinLock<Sched> = OnceSpinLock::new();

extern "C" fn thread_wrapper(f: extern "C" fn(*mut ()), arg: *mut ()) {
    f(arg);
    SCHEDULER.get().unwrap().end_task();

    let next_task = SCHEDULER.get().unwrap().next_task().unwrap();

    restore_regs_and_eret(next_task);
}

pub fn init_scheduler() {
    assert!(
        SCHEDULER
            .set(Sched {
                run_queue: SpinLock::new(List::new()),
                blocked_queue: SpinLock::new(List::new()),
                kill_queue: SpinLock::new(List::new()),
                scheduled: SpinLock::new(None),
            })
            .is_ok(),
        "Couldn't initialize scheduler"
    );
}

impl_link!(Task, 0 => links);

pub struct Sched {
    // Tasks that are available to run.
    run_queue: SpinLock<List<Task>>,
    // Tasks that are blocked.
    blocked_queue: SpinLock<List<Task>>,
    // Tasks to be killed on the next tick.
    kill_queue: SpinLock<List<Task>>,
    scheduled: SpinLock<Option<Arc<Task>>>,
}

fn create_stack() -> KBox<TaskStack> {
    let b = KBox::new_zeroed_in(&KERNEL_ALLOCATOR);
    unsafe { b.assume_init() }
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
        "stp x2, x29, [x0, #0x60]",
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

        let this_task = SCHEDULER.get().unwrap().task().unwrap();

        let mut regs = this_task.registers.lock();
        let reg_ptr = unsafe { regs.gprs.as_mut_ptr().add(18) };
        save_callee_regs(reg_ptr, addr);
        drop(regs);
    }

    let next_task = SCHEDULER.get().unwrap().next_task().unwrap();

    restore_regs_and_eret(next_task);

    unsafe {
        asm!("1: nop");
    }

    daifclr();
}

impl Sched {
    pub fn task(&self) -> Option<Arc<Task>> {
        self.scheduled.lock().as_ref().cloned()
    }

    pub fn block_this_task(&self) {
        let task = self.task().unwrap();

        let task = unsafe { self.run_queue.lock().remove_at(&task) };

        self.blocked_queue.lock().push_back(task);
    }

    pub fn unblock_task(&self, task: &Arc<Task>) {
        let task = unsafe { self.blocked_queue.lock().remove_at(task) };

        self.run_queue.lock().push_back(task);
    }

    pub fn end_task(&self) {
        let mut tasks = self.run_queue.lock();
        let mut kill_queue = self.kill_queue.lock();
        let mut current = self.scheduled.lock();

        if let Some(task) = current.take() {
            let task = unsafe { tasks.remove_at(&task) };
            kill_queue.push_back(task);
        }
    }

    #[allow(clippy::fn_to_numeric_cast, function_casts_as_integer)]
    pub fn task_from_fn(&self, f: fn(*mut ()), arg: *mut ()) {
        let task = UniqueArc::new(Task {
            registers: SpinLock::new(ExceptionRegisters::default()),
            links: ListLinks::new(),
            stack: create_stack(),
        });

        let mut registers = task.registers.lock();
        registers.elr = (thread_wrapper) as u64;
        registers.gprs[0] = (f as usize) as u64;
        registers.gprs[1] = arg as u64;
        registers.gprs[31] = task.stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64;
        assert!(
            registers.gprs[31].is_multiple_of(16),
            "Stack ptr is not aligned"
        );
        registers.spsr = 0b0101;
        drop(registers);

        self.run_queue.lock().push_back(task.into());
    }

    pub fn save_register_state_to_task(&self, state: &ExceptionRegisters) {
        let Some(task) = self.task() else {
            return;
        };
        let mut registers = task.registers.lock();
        *registers = state.clone();
    }

    pub fn next_task(&self) -> Option<*const ExceptionRegisters> {
        let mut kill_queue = self.kill_queue.lock();

        while !kill_queue.is_empty() {
            kill_queue.remove_front();
        }

        drop(kill_queue);

        let mut tasks = self.run_queue.lock();
        let task = tasks.remove_front();

        if let Some(task) = task {
            let mut scheduled = self.scheduled.lock();
            *scheduled = Some(task.clone_arc());

            tasks.push_back(task);

            Some(&raw const *scheduled.as_ref().unwrap().registers.lock())
        } else {
            None
        }
    }
}
