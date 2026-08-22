use core::sync::atomic::{AtomicUsize, Ordering::SeqCst};

use crate::{
    allocators::{KBox, KERNEL_ALLOCATOR, KVec, kvec},
    elf::Segment,
    impl_link,
    interrupts::ExceptionRegisters,
    sched::Mutex,
    subsystem::{ArmPageTableRoot, Inode},
    utils::{Arc, ListLinks, SpinLock},
};

pub(crate) const STACK_SIZE: usize = 4096 * 16;

#[repr(align(16))]
pub(crate) struct KernelTaskStack([u8; STACK_SIZE]);

impl KernelTaskStack {
    pub fn phys_addr(&self) -> usize {
        self.0.as_ptr().addr() & 0x7fffffffff
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[repr(align(4096))]
pub(crate) struct UserTaskStack([u8; STACK_SIZE]);

impl UserTaskStack {
    pub fn phys_addr(&self) -> usize {
        self.0.as_ptr().addr() & 0x7fffffffff
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

pub struct UserSpaceTaskInfo {
    pub(crate) page_table: SpinLock<ArmPageTableRoot>,
    pub(crate) user_stack: KBox<UserTaskStack>,
    pub(crate) kernel_stack: KBox<KernelTaskStack>,
    pub(crate) segments: KVec<Segment>,
    pub(crate) fds: TaskFdTable,
}

pub struct KernelSpaceTaskInfo {
    pub(crate) kernel_stack: KBox<KernelTaskStack>,
}

#[allow(clippy::large_enum_variant)]
pub enum TaskInfo {
    Kernel(KernelSpaceTaskInfo),
    User(UserSpaceTaskInfo),
}

impl TaskInfo {
    pub fn new_kernel(stack: KBox<KernelTaskStack>) -> Self {
        Self::Kernel(KernelSpaceTaskInfo {
            kernel_stack: stack,
        })
    }
}

#[repr(align(16))]
pub struct Task {
    pub(crate) registers: SpinLock<ExceptionRegisters>,
    pub(crate) links: ListLinks,
    pub(crate) task_info: Arc<TaskInfo>,
}

impl Task {
    pub fn stack_top(&self) -> u64 {
        match *self.task_info {
            TaskInfo::Kernel(ref kernel_space_task_info) => {
                kernel_space_task_info.kernel_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
            TaskInfo::User(ref user_space_task_info) => {
                user_space_task_info.user_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
        }
    }

    pub fn kernel_stack_top(&self) -> u64 {
        match *self.task_info {
            TaskInfo::Kernel(ref kernel_space_task_info) => {
                kernel_space_task_info.kernel_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
            TaskInfo::User(ref user_space_task_info) => {
                user_space_task_info.kernel_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
        }
    }

    pub fn user_fd_table(&self) -> Option<&TaskFdTable> {
        match *self.task_info {
            TaskInfo::Kernel(_) => None,
            TaskInfo::User(ref user_space_task_info) => Some(&user_space_task_info.fds),
        }
    }
}

pub(crate) fn create_kernel_stack() -> KBox<KernelTaskStack> {
    let b = KBox::new_zeroed_in(&KERNEL_ALLOCATOR);
    unsafe { b.assume_init() }
}

pub(crate) fn create_user_stack() -> KBox<UserTaskStack> {
    let b = KBox::new_zeroed_in(&KERNEL_ALLOCATOR);
    unsafe { b.assume_init() }
}

impl_link!(Task, 0 => links);

pub struct TaskFdTable {
    fds: Mutex<KVec<TaskFd>>,
    next_fd_number: AtomicUsize,
}

impl TaskFdTable {
    pub fn new() -> Self {
        Self {
            fds: Mutex::new(kvec()),
            next_fd_number: AtomicUsize::new(11),
        }
    }

    pub fn add_file_fd(&self, inode: Arc<Inode>) -> usize {
        let mut fds = self.fds.lock();
        let descriptor = self.next_fd_number.fetch_add(1, SeqCst);

        fds.push(TaskFd::File {
            descriptor,
            inode,
            offset: AtomicUsize::new(0),
        });

        descriptor
    }

    pub fn read(&self, descriptor: usize, buf: &mut [u8]) -> isize {
        let fds = self.fds.lock();
        let Some(fd) = fds.iter().find(|fd| fd.descriptor() == descriptor) else {
            return -1;
        };

        fd.read(buf)
    }

    pub fn write(&self, descriptor: usize, buf: &[u8]) -> isize {
        let fds = self.fds.lock();
        let Some(fd) = fds.iter().find(|fd| fd.descriptor() == descriptor) else {
            return -1;
        };

        fd.write(buf)
    }

    pub fn close(&self, descriptor: usize) {
        let mut fds = self.fds.lock();

        fds.retain(|fd| fd.descriptor() != descriptor);
    }
}

enum TaskFd {
    File {
        descriptor: usize,
        inode: Arc<Inode>,
        offset: AtomicUsize,
    },
}

impl TaskFd {
    pub fn descriptor(&self) -> usize {
        match self {
            TaskFd::File {
                descriptor,
                inode: _,
                offset: _,
            } => *descriptor,
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> isize {
        match self {
            TaskFd::File {
                descriptor: _,
                inode,
                offset,
            } => {
                let local_offset = offset.load(SeqCst);
                if let Ok(read) = inode.read(local_offset as u64, buf) {
                    offset.fetch_add(read, SeqCst);

                    read as isize
                } else {
                    -1
                }
            }
        }
    }

    pub fn write(&self, buf: &[u8]) -> isize {
        match self {
            TaskFd::File {
                descriptor: _,
                inode,
                offset,
            } => {
                let local_offset = offset.load(SeqCst);
                if let Ok(()) = inode.write(local_offset as u64, buf) {
                    offset.fetch_add(buf.len(), SeqCst);

                    buf.len() as isize
                } else {
                    -1
                }
            }
        }
    }
}
