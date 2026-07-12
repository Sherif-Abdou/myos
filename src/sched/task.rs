use core::arch::asm;

use crate::{
    allocators::{KBox, KERNEL_ALLOCATOR}, early_printk, impl_link, interrupts::{ExceptionRegisters, daifclr}, utils::{Arc, List, ListLinks, OnceSpinLock, SpinLock, UniqueArc},
};

const STACK_SIZE: usize = 4096 * 4;

#[repr(align(16))]
pub struct Task {
    registers: ExceptionRegisters,
    links: ListLinks,
    stack: KBox<[u8; STACK_SIZE]>,
}

pub static SCHEDULER: OnceSpinLock<Sched> = OnceSpinLock::new();

fn loop1(_arg: *mut ()) {
    loop {
        daifclr();
        early_printk!("Hello from thread 1\n");
        unsafe {
            asm!("wfi");
        }
    }
}

fn loop2(_arg: *mut ()) {
    loop {
        daifclr();
        early_printk!("Hello from thread 2\n");
        unsafe {
            asm!("wfi");
        }
    }
}

pub fn init_scheduler() {
    assert!(
        SCHEDULER
            .set(Sched {
                tasks: SpinLock::new(List::new()),
                scheduled: SpinLock::new(None),
            })
            .is_ok(),
        "Couldn't initialize scheduler"
    );
    SCHEDULER.get().unwrap().task_from_fn(loop1);
    SCHEDULER.get().unwrap().task_from_fn(loop2);
}

impl_link!(Task, 0 => links);

pub struct Sched {
    tasks: SpinLock<List<Task>>,
    scheduled: SpinLock<Option<Arc<Task>>>,
}

pub fn create_stack() -> KBox<[u8; STACK_SIZE]> {
    let b = KBox::new_zeroed_in(&KERNEL_ALLOCATOR);
    unsafe { b.assume_init() }
}

impl Sched {
    pub fn task_from_fn(&self, f: fn(*mut ())) {
        let mut task = UniqueArc::new(Task {
            registers: ExceptionRegisters::default(),
            links: ListLinks::new(),
            stack: create_stack(),
        });

        task.registers.elr = (f as usize) as u64;
        task.registers.gprs[31] = task.stack.as_ptr().addr() as u64 + STACK_SIZE as u64;
        task.registers.spsr = 0b0101;

        self.tasks.lock().push_back(task.into());
    }

    pub fn next_task(&self) -> Option<*const ExceptionRegisters> {
        let mut tasks = self.tasks.lock();
        let task = tasks.remove_front();
        if let Some(task) = task {
            let mut scheduled = self.scheduled.lock();
            *scheduled = Some(task.clone_arc());

            tasks.push_back(task);

            Some(&raw const scheduled.as_ref().unwrap().registers)
        } else {
            None
        }
    }
}
