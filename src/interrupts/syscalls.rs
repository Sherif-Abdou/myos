use core::str;

use crate::{
    allocators::{KBox, align_up, kbox_bytes, kbox_with_len},
    interrupts::{ExceptionRegisters, RETURN_TABLE, daifset, sexc_handler::UserInput},
    sched::SCHEDULER,
    subsystem::{FileSystem, InodeOperations, MOUNT_TABLE, Pipe},
    timer::us_sleep,
    utils::Arc,
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

pub fn validate_user_address_range(addr: usize, len: usize) -> bool {
    (addr < 0x7fffffffff) && (addr.saturating_add(len) < 0x7fffffffff)
}

pub fn write(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let descriptor = unsafe { (*regs).gprs[0] } as usize;
    let addr = unsafe { (*regs).gprs[1] } as usize;
    let len = unsafe { (*regs).gprs[2] } as usize;

    let user_buf = unsafe { UserInput::from_raw_parts(addr, len) };
    let Ok(user_buf) = user_buf else {
        unsafe {
            (*regs).gprs[0] = -1i64 as u64;
        }

        return regs;
    };

    let mut kernel_buf = kbox_bytes(user_buf.len());

    let len = user_buf.copy_to_slice(&mut kernel_buf[..len]);

    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    let ret = task
        .user_fd_table()
        .unwrap()
        .write(descriptor, &kernel_buf[..len]);

    unsafe {
        (*regs).gprs[0] = ret as u64;
    }

    regs
}

pub fn read(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let descriptor = unsafe { (*regs).gprs[0] } as usize;
    let addr = unsafe { (*regs).gprs[1] } as usize;
    let len = unsafe { (*regs).gprs[2] } as usize;
    let user_buf = unsafe { UserInput::from_raw_parts_mut(addr, len) };
    let Ok(mut user_buf) = user_buf else {
        unsafe {
            (*regs).gprs[0] = -1i64 as u64;
        }

        return regs;
    };

    let mut scratch = kbox_bytes(len);

    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    let ret = task.user_fd_table().unwrap().read(descriptor, &mut scratch);

    if ret < 0 {
        unsafe {
            (*regs).gprs[0] = ret as u64;
        }
    } else {
        let len = ret as usize;
        let len = user_buf.copy_from_slice(&scratch[..len]);
        unsafe {
            (*regs).gprs[0] = len as u64;
        }
    }

    regs
}

const MAX_STRING_LEN: usize = 2048;

pub fn open(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let user_cstr_addr = unsafe { (*regs).gprs[0] } as usize;
    let user_cstr = unsafe { UserInput::from_cstr(user_cstr_addr, MAX_STRING_LEN) };

    let Ok(user_cstr) = user_cstr else {
        unsafe {
            (*regs).gprs[0] = -1i64 as u64;
        }
        return regs;
    };

    let mut scratch = kbox_bytes(user_cstr.len());

    user_cstr.copy_to_slice(&mut scratch);

    let Ok(path) = str::from_utf8(&scratch) else {
        unsafe {
            (*regs).gprs[0] = -1i64 as u64;
        }
        return regs;
    };

    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    let inode = MOUNT_TABLE.get().unwrap().open(path);

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

pub fn parse_argv(argc: u64, argv: &[u64]) -> Result<KBox<[u8]>, ()> {
    // 4 bytes for argc
    let mut buffer_size = 8 + 8 * argc as usize;

    for i in 0..argc {
        let local_argv = unsafe { UserInput::from_cstr(argv[i as usize] as usize, MAX_STRING_LEN) };
        let Ok(strlen) = local_argv.map(|input| input.len()) else {
            return Err(());
        };

        buffer_size += strlen + 1;
    }

    // Ensure buffer is aligned to a size that can be placed on the stack.
    let mut buffer = kbox_bytes(align_up(buffer_size, 16));
    let mut index = 8 + 8 * argc as usize;

    buffer[0..8].copy_from_slice(&argc.to_le_bytes());

    for i in 0..argc {
        let local_argv = unsafe { UserInput::from_cstr(argv[i as usize] as usize, MAX_STRING_LEN) };

        let Ok(local_argv) = local_argv else {
            return Err(());
        };

        let strlen = local_argv.len();
        local_argv.copy_to_slice(&mut buffer[index..(index + strlen)]);

        buffer[index + strlen] = 0;

        buffer[8 * (i as usize + 1)..8 * (i as usize + 2)].copy_from_slice(&index.to_le_bytes());

        index += strlen + 1;
    }

    Ok(buffer)
}

pub fn exec(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let user_cstr_addr = unsafe { (*regs).gprs[0] } as usize;
    let user_cstr = unsafe { UserInput::from_cstr(user_cstr_addr, MAX_STRING_LEN) };

    let Ok(user_cstr) = user_cstr else {
        unsafe {
            (*regs).gprs[0] = -1i64 as u64;
        }
        return regs;
    };

    let argc = unsafe { (*regs).gprs[1] };
    let argv_addr = unsafe { (*regs).gprs[2] };

    let mut scratch = kbox_bytes(user_cstr.len());

    user_cstr.copy_to_slice(&mut scratch);

    let path = str::from_utf8(&scratch).unwrap();

    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    let inode = MOUNT_TABLE.get().unwrap().open(path);

    if let Ok(inode) = inode {
        let user_argv =
            unsafe { UserInput::<&[u64]>::from_raw_parts(argv_addr as usize, argc as usize) };
        let Ok(user_argv) = user_argv else {
            unsafe {
                (*regs).gprs[0] = -1i64 as u64;
            }
            return regs;
        };

        let mut scratch_argv = kbox_with_len::<u64>(argc as usize);
        let len_copied = user_argv.copy_to_slice(&mut scratch_argv);

        if len_copied != argc as usize {
            unsafe {
                (*regs).gprs[0] = -1i64 as u64;
            }
            return regs;
        }

        let Ok(args) = parse_argv(argc, &scratch_argv) else {
            unsafe {
                (*regs).gprs[0] = -1i64 as u64;
            }
            return regs;
        };

        let new_regs = task.exec(Arc::new(inode), &args);
        task.bind_pages();

        unsafe { (*regs) = new_regs };
    } else {
        unsafe {
            (*regs).gprs[0] = (-1i64) as u64;
        }
    }

    regs
}

pub fn dup2(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let old_fd = unsafe { (*regs).gprs[0] };
    let new_fd = unsafe { (*regs).gprs[1] };

    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    if task
        .user_fd_table()
        .unwrap()
        .dup_fd(new_fd as u32, old_fd as u32)
        .is_ok()
    {
        unsafe {
            (*regs).gprs[0] = 0i64 as u64;
        }
    } else {
        unsafe {
            (*regs).gprs[0] = -1i64 as u64;
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

pub fn pipe(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let user_output = unsafe { UserInput::from_raw_parts_mut((*regs).gprs[0] as usize, 2) };
    let Ok(mut user_output) = user_output else {
        unsafe {
            (*regs).gprs[0] = -1i64 as u64;
        }

        return regs;
    };

    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    let pipe: Arc<dyn InodeOperations> = Arc::new(Pipe::new());

    let descriptor1 = task.user_fd_table().unwrap().add_anon_file_fd(pipe.clone()) as u32;
    let descriptor2 = task.user_fd_table().unwrap().add_anon_file_fd(pipe.clone()) as u32;

    user_output.copy_from_slice(&[descriptor1, descriptor2]);

    unsafe {
        (*regs).gprs[0] = 0;
    }

    regs
}

pub fn sbrk(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let task = SCHEDULER.get().unwrap().local_task().unwrap();

    let offset = unsafe { (*regs).gprs[0] as isize };

    if let Ok(ret) = task.offset_heap(offset) {
        unsafe {
            (*regs).gprs[0] = ret as u64;
        }
    } else {
        unsafe {
            (*regs).gprs[0] = (-1i64) as u64;
        }
    }

    regs
}

const fn build_syscall_table() -> [Syscall; 100] {
    let mut table = [const { Syscall::empty() }; 100];

    table[0] = Syscall::new(write);
    table[1] = Syscall::new(read);
    table[8] = Syscall::new(open);
    table[11] = Syscall::new(close);
    table[14] = Syscall::new(dup2);
    table[17] = Syscall::new(nanosleep);
    table[20] = Syscall::new(fork);
    table[22] = Syscall::new(exec);
    table[27] = Syscall::new(waitpid);
    table[33] = Syscall::new(sbrk);
    table[40] = Syscall::new(pipe);
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
    let Some(call) = SYSCALL_TABLE.get(num as usize) else {
        unsafe {
            (*regs).gprs[0] = -1i64 as u64;
        }

        return regs;
    };

    if let Some(func) = call.func {
        func(regs)
    } else {
        regs
    }
}
