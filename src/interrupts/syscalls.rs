use core::str;

use alloc::{slice};

use crate::{interrupts::ExceptionRegisters, printk, sched::SCHEDULER, subsystem::CONSOLE};

pub(crate) struct Syscall {
    func: Option<fn(*mut ExceptionRegisters) -> *const ExceptionRegisters>,
}

impl Syscall {
    pub(crate) const fn empty() -> Self {
        Self { func: None }
    }

    pub(crate) const fn new(
        func: fn(*mut ExceptionRegisters) -> *const ExceptionRegisters,
    ) -> Self {
        Self { func: Some(func) }
    }
}

pub fn write(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let addr = unsafe { (*regs).gprs[0] } as *mut u8;
    let len = unsafe { (*regs).gprs[1] } as usize;

    let str = unsafe { str::from_utf8(slice::from_raw_parts(addr, len)).unwrap() };

    printk!("{}", str);

    unsafe {
        (*regs).gprs[0] = str.len() as u64;
    }

    regs
}

pub fn read(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let addr = unsafe { (*regs).gprs[0] } as *mut u8;
    let size = unsafe { (*regs).gprs[1] } as usize;

    let buf = unsafe { slice::from_raw_parts_mut(addr, size) };

    let len = CONSOLE.get().unwrap().read(buf);

    unsafe {
        (*regs).gprs[0] = len as u64;
    }

    regs
}

pub fn exit(_regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    SCHEDULER.get().unwrap().end_task();

    SCHEDULER.get().unwrap().next_task().unwrap()
}

const fn build_syscall_table() -> [Syscall; 100] {
    let mut table = [const { Syscall::empty() }; 100];

    table[0] = Syscall::new(write);
    table[1] = Syscall::new(read);
    table[50] = Syscall::new(exit);

    table
}

static SYSCALL_TABLE: &[Syscall] = &build_syscall_table();

pub fn dispatch_syscall(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let num = unsafe { (*regs).gprs[8] };
    let call = &SYSCALL_TABLE[num as usize];

    if let Some(func) = call.func {
        func(regs)
    } else {
        regs
    }
}
