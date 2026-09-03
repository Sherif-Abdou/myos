use core::str;

use alloc::slice;

use crate::{
    allocators::{KBox, align_up, kbox_with_len},
    interrupts::{
        ExceptionRegisters, RETURN_TABLE, daifset,
        sexc_handler::{copy_from_user, copy_to_user, user_strlen},
    },
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

fn validate_user_address_range(addr: usize, len: usize) -> bool {
    (addr < 0x7fffffffff) && (addr + len < 0x7fffffffff)
}

pub fn write(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let descriptor = unsafe { (*regs).gprs[0] } as usize;
    let addr = unsafe { (*regs).gprs[1] } as *mut u8;
    let len = unsafe { (*regs).gprs[2] } as usize;

    let mut kernel_buf = kbox_with_len(len);

    if descriptor == 0 {
        let user_buf = unsafe { slice::from_raw_parts(addr, len) };
        let len = copy_from_user(&mut kernel_buf[..len], &user_buf[..len]);
        let str = str::from_utf8(&kernel_buf[..len]).unwrap();

        printk!("{}", str);

        unsafe {
            (*regs).gprs[0] = str.len() as u64;
        }
    } else {
        let user_buf = unsafe { slice::from_raw_parts(addr, len) };

        let len = copy_from_user(&mut kernel_buf[..len], &user_buf[..len]);

        let task = SCHEDULER.get().unwrap().local_task().unwrap();

        let ret = task
            .user_fd_table()
            .unwrap()
            .write(descriptor, &kernel_buf[..len]);

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
    let user_buf = unsafe { slice::from_raw_parts_mut(addr, len) };

    let mut scratch = kbox_with_len(len);

    if descriptor == 1 {
        let len = CONSOLE.get().unwrap().read(&mut scratch);

        let len = copy_to_user(&mut user_buf[..len], &scratch[..len]);

        unsafe {
            (*regs).gprs[0] = len as u64;
        }
    } else {
        let task = SCHEDULER.get().unwrap().local_task().unwrap();

        let ret = task.user_fd_table().unwrap().read(descriptor, &mut scratch);

        if ret < 0 {
            unsafe {
                (*regs).gprs[0] = ret as u64;
            }
        } else {
            let len = ret as usize;
            let len = copy_to_user(&mut user_buf[..len], &scratch[..len]);

            unsafe {
                (*regs).gprs[0] = len as u64;
            }
        }
    }

    regs
}

pub fn open(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let user_cstr_addr = unsafe { (*regs).gprs[0] } as *mut u8;
    let user_cstr_len = user_strlen(user_cstr_addr.cast_const());

    let mut scratch = kbox_with_len(user_cstr_len);

    copy_from_user(&mut scratch, unsafe {
        slice::from_raw_parts_mut(user_cstr_addr, user_cstr_len)
    });

    let path = str::from_utf8(&scratch).unwrap();

    let task = SCHEDULER.get().unwrap().local_task().unwrap();

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

pub fn parse_argv(argc: u64, argv_addr: u64) -> KBox<[u8]> {
    // 4 bytes for argc
    let mut buffer_size = 8 + 8 * argc as usize;

    let argv = argv_addr as *mut *mut u8;
    for i in 0..argc {
        let local_argv = unsafe { argv.add(i as usize).read() };
        let strlen = user_strlen(local_argv);

        buffer_size += strlen + 1;
    }

    // Ensure buffer is aligned to a size that can be placed on the stack.
    let mut buffer = kbox_with_len(align_up(buffer_size, 16));
    let mut index = 8 + 8 * argc as usize;

    buffer[0..8].copy_from_slice(&argc.to_le_bytes());

    for i in 0..argc {
        let local_argv = unsafe { argv.add(i as usize).read() };
        let strlen = user_strlen(local_argv);
        let src = unsafe {
            core::ptr::slice_from_raw_parts(local_argv, strlen)
                .as_ref()
                .unwrap()
        };
        copy_from_user(&mut buffer[index..(index + strlen)], src);
        buffer[index + strlen] = 0;

        buffer[8 * (i as usize + 1)..8 * (i as usize + 2)].copy_from_slice(&index.to_le_bytes());

        index += strlen + 1;
    }

    buffer
}

pub fn exec(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let user_cstr_addr = unsafe { (*regs).gprs[0] } as *mut u8;
    let user_cstr_len = user_strlen(user_cstr_addr.cast_const());
    let argc = unsafe { (*regs).gprs[1] };
    let argv_addr = unsafe { (*regs).gprs[2] };

    let mut scratch = kbox_with_len(user_cstr_len);

    copy_from_user(&mut scratch, unsafe {
        slice::from_raw_parts_mut(user_cstr_addr, user_cstr_len)
    });

    let path = str::from_utf8(&scratch).unwrap();

    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    let inode = EXT2_FS.get().unwrap().open(path);

    if let Ok(inode) = inode {
        let mut scratch_argv = kbox_with_len(8 * argc as usize);
        let bytes = copy_from_user(&mut scratch_argv, unsafe {
            core::slice::from_raw_parts(argv_addr as _, 8 * argc as usize)
        });

        let args = parse_argv(argc, (*scratch_argv).as_ptr().addr() as u64);
        let new_regs = task.exec(&*inode, &args);
        task.bind_pages();

        unsafe { (*regs) = new_regs };
    } else {
        unsafe {
            (*regs).gprs[0] = (-1i64) as u64;
        }
    }

    regs
}

pub fn close(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let descriptor = unsafe { (*regs).gprs[0] };

    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    task.user_fd_table().unwrap().close(descriptor as usize);

    unsafe {
        (*regs).gprs[0] = 0;
    }

    regs
}

pub fn exit(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let code = unsafe { (*regs).gprs[0] as i32 };
    SCHEDULER.get().unwrap().end_task(code);

    daifset();
    // TODO: Use a separate sexc table
    RETURN_TABLE
        .lock()
        .put(&SCHEDULER.get().unwrap().next_task().unwrap());

    RETURN_TABLE.lock().take().unwrap()
}

pub fn waitpid(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let pid = unsafe { (*regs).gprs[0] as u32 };

    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    if let Some(child) = task.find_child(pid) {
        if child
            .completion_waiters
            .prepare_enqueue(|| !child.is_done())
        {
            child.completion_waiters.block();
        }
        unsafe {
            (*regs).gprs[0] = 0;
        }
    } else {
        unsafe {
            (*regs).gprs[0] = -1i64 as u64;
        }
    }

    regs
}

pub fn nanosleep(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let delay_ns = unsafe { (*regs).gprs[0] };

    us_sleep(delay_ns / 1000);

    unsafe {
        (*regs).gprs[0] = 0;
    }

    regs
}

pub fn fork(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    let pid = task.fork_process();

    unsafe {
        (*regs).gprs[0] = pid as u64;
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
    table[20] = Syscall::new(fork);
    table[22] = Syscall::new(exec);
    table[27] = Syscall::new(waitpid);
    table[50] = Syscall::new(exit);

    table
}

static SYSCALL_TABLE: &[Syscall] = &build_syscall_table();

pub fn dispatch_syscall(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    SCHEDULER
        .get()
        .unwrap()
        .save_register_state_to_task(unsafe { regs.as_ref().unwrap() });

    let num = unsafe { (*regs).gprs[8] };
    let call = &SYSCALL_TABLE[num as usize];

    if let Some(func) = call.func {
        func(regs)
    } else {
        regs
    }
}
