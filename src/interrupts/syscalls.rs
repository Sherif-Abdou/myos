use core::{ffi::CStr, str};

use alloc::slice;

use crate::{
    interrupts::ExceptionRegisters,
    printk,
    sched::SCHEDULER,
    subsystem::{CONSOLE, EXT2_FS, FileSystem},
    timer::us_sleep,
};

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
    let descriptor = unsafe { (*regs).gprs[0] } as usize;
    let addr = unsafe { (*regs).gprs[1] } as *mut u8;
    let len = unsafe { (*regs).gprs[2] } as usize;

    if descriptor == 0 {
        let str = unsafe { str::from_utf8(slice::from_raw_parts(addr, len)).unwrap() };

        printk!("{}", str);

        unsafe {
            (*regs).gprs[0] = str.len() as u64;
        }
    } else {
        let buf = unsafe { slice::from_raw_parts(addr, len) };

        let task = SCHEDULER.get().unwrap().task().unwrap();

        let ret = task.user_fd_table().unwrap().write(descriptor, buf);

        unsafe {
            (*regs).gprs[0] = ret as u64;
        }
    }

    regs
}

pub fn read(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let descriptor = unsafe { (*regs).gprs[0] } as usize;
    let addr = unsafe { (*regs).gprs[1] } as *mut u8;
    let len = unsafe { (*regs).gprs[2] } as usize;
    let buf = unsafe { slice::from_raw_parts_mut(addr, len) };

    if descriptor == 1 {
        let len = CONSOLE.get().unwrap().read(buf);

        unsafe {
            (*regs).gprs[0] = len as u64;
        }
    } else {
        let task = SCHEDULER.get().unwrap().task().unwrap();

        let ret = task.user_fd_table().unwrap().read(descriptor, buf);

        unsafe {
            (*regs).gprs[0] = ret as u64;
        }
    }

    regs
}

pub fn open(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let cstr_addr = unsafe { (*regs).gprs[0] } as *mut u8;

    let buf = unsafe { CStr::from_ptr(cstr_addr) };

    let path = buf.to_str().unwrap();

    let task = SCHEDULER.get().unwrap().task().unwrap();

    let inode = EXT2_FS.get().unwrap().open(path);

    if let Ok(inode) = inode {
        let descriptor = task.user_fd_table().unwrap().add_file_fd(inode);
        unsafe {
            (*regs).gprs[0] = descriptor as u64;
        }
    } else {
        unsafe {
            (*regs).gprs[0] = (-1i64) as u64;
        }
    }

    regs
}

pub fn close(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let descriptor = unsafe { (*regs).gprs[0] };

    let task = SCHEDULER.get().unwrap().task().unwrap();

    task.user_fd_table().unwrap().close(descriptor as usize);

    unsafe {
        (*regs).gprs[0] = 0;
    }

    regs
}

pub fn exit(_regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    SCHEDULER.get().unwrap().end_task();

    SCHEDULER.get().unwrap().next_task().unwrap()
}

pub fn nanosleep(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let delay_ns = unsafe { (*regs).gprs[0] };

    us_sleep(delay_ns / 1000);

    unsafe {
        (*regs).gprs[0] = 0;
    }

    regs
}

const fn build_syscall_table() -> [Syscall; 100] {
    let mut table = [const { Syscall::empty() }; 100];

    table[0] = Syscall::new(write);
    table[1] = Syscall::new(read);
    table[8] = Syscall::new(open);
    table[11] = Syscall::new(close);
    table[17] = Syscall::new(nanosleep);
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
