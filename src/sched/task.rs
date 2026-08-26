use core::sync::atomic::{AtomicUsize, Ordering::SeqCst};

use crate::{
    allocators::{KBox, KERNEL_ALLOCATOR, KVec, kvec},
    elf::Segment,
    impl_link,
    interrupts::ExceptionRegisters,
    printk,
    sched::{Mutex, SCHEDULER, STACK_VIRTUAL_ADDR},
    subsystem::{ArmPageTableRoot, Inode},
    utils::{Arc, ListLinks, PhysAddr, SpinLock, UniqueArc},
};

pub(crate) const STACK_SIZE: usize = 4096 * 16;

#[repr(align(16))]
pub(crate) struct KernelTaskStack([u8; STACK_SIZE]);

impl KernelTaskStack {
    pub fn clone_box(boxed: &KBox<Self>) -> KBox<Self> {
        let mut new_stack: KBox<Self> =
            unsafe { KBox::new_zeroed_in(&KERNEL_ALLOCATOR).assume_init() };

        new_stack.0.copy_from_slice(boxed.0.as_slice());

        new_stack
    }

    pub fn phys_addr(&self) -> PhysAddr {
        PhysAddr::from(self.0.as_ptr())
    }

    pub fn virt_addr(&self) -> usize {
        self.0.as_ptr().addr()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[repr(align(4096))]
pub(crate) struct UserTaskStack([u8; STACK_SIZE]);

impl UserTaskStack {
    pub fn clone_box(boxed: &KBox<Self>) -> KBox<Self> {
        let mut new_stack: KBox<UserTaskStack> =
            unsafe { KBox::new_zeroed_in(&KERNEL_ALLOCATOR).assume_init() };

        new_stack.0.copy_from_slice(boxed.0.as_slice());

        new_stack
    }

    pub fn phys_addr(&self) -> PhysAddr {
        PhysAddr::from(self.0.as_ptr())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

pub struct UserSpaceProcess {
    pub(crate) page_table: SpinLock<ArmPageTableRoot>,
    pub(crate) user_stack: KBox<UserTaskStack>,
    pub(crate) kernel_stack: KBox<KernelTaskStack>,
    pub(crate) segments: KVec<Segment>,
    pub(crate) fds: TaskFdTable,
}

impl UserSpaceProcess {
    fn fork(&self) -> UserSpaceProcess {
        let new_page_table = ArmPageTableRoot::create_user();

        let new_segments = self.segments.clone();

        let new_user_stack = UserTaskStack::clone_box(&self.user_stack);
        let new_kernel_stack = KernelTaskStack::clone_box(&self.kernel_stack);
        let new_fds = self.fds.fork();

        for segment in new_segments.iter() {
            if !segment.is_null() {
                let phys_addr = segment.loaded_phys_addr();
                let virt_addr = segment.virt_addr() & !0xfff;
                let pages = segment.mem_page_count();

                new_page_table.map_page_range(virt_addr, phys_addr, pages);
            }
        }

        new_page_table.map_page_range(
            STACK_VIRTUAL_ADDR,
            new_user_stack.phys_addr(),
            new_user_stack.len().div_ceil(4096),
        );

        UserSpaceProcess {
            page_table: SpinLock::new(new_page_table),
            user_stack: new_user_stack,
            kernel_stack: new_kernel_stack,
            segments: new_segments,
            fds: new_fds,
        }
    }
}

pub struct KernelSpaceProcess {
    pub(crate) kernel_stack: KBox<KernelTaskStack>,
}

impl KernelSpaceProcess {
    pub fn fork(&self) -> KernelSpaceProcess {
        KernelSpaceProcess {
            kernel_stack: KernelTaskStack::clone_box(&self.kernel_stack),
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum Process {
    Kernel(KernelSpaceProcess),
    User(UserSpaceProcess),
}

impl Process {
    pub fn new_kernel(stack: KBox<KernelTaskStack>) -> Self {
        Self::Kernel(KernelSpaceProcess {
            kernel_stack: stack,
        })
    }

    pub fn fork(&self) -> Process {
        match self {
            Process::Kernel(kernel_space_process) => Process::Kernel(kernel_space_process.fork()),
            Process::User(user_space_process) => Process::User(user_space_process.fork()),
        }
    }
}

#[repr(align(16))]
pub struct Task {
    pub(crate) registers: SpinLock<ExceptionRegisters>,
    pub(crate) links: ListLinks,
    pub(crate) process: Arc<Process>,
}

impl Task {
    pub fn fork_process(&self) {
        let registers = self.registers.lock();
        let mut new_registers = registers.clone();
        let new_process = Arc::new(self.process.fork());
        drop(registers);

        let old_kernel_stack = self.kernel_stack_top();
        let new_kernel_stack = match *new_process {
            Process::Kernel(ref kernel_space_task_info) => {
                kernel_space_task_info.kernel_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
            Process::User(ref user_space_task_info) => {
                user_space_task_info.kernel_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
        };

        new_registers.gprs[0] = 0;
        new_registers.gprs[31] = new_kernel_stack - (old_kernel_stack - new_registers.gprs[31]);

        let new_task = UniqueArc::new(Task {
            registers: SpinLock::new(new_registers),
            links: ListLinks::new(),
            process: new_process,
        });

        SCHEDULER.get().unwrap().append_task(new_task);
    }

    pub fn stack_top(&self) -> u64 {
        match *self.process {
            Process::Kernel(ref kernel_space_task_info) => {
                kernel_space_task_info.kernel_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
            Process::User(ref user_space_task_info) => {
                user_space_task_info.user_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
        }
    }

    pub fn kernel_stack_top(&self) -> u64 {
        match *self.process {
            Process::Kernel(ref kernel_space_task_info) => {
                kernel_space_task_info.kernel_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
            Process::User(ref user_space_task_info) => {
                user_space_task_info.kernel_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
        }
    }

    pub fn user_fd_table(&self) -> Option<&TaskFdTable> {
        match *self.process {
            Process::Kernel(_) => None,
            Process::User(ref user_space_task_info) => Some(&user_space_task_info.fds),
        }
    }

    pub fn is_user_task(&self) -> bool {
        match *self.process {
            Process::Kernel(_) => false,
            Process::User(_) => true,
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

    pub fn fork(&self) -> TaskFdTable {
        let list = self.fds.lock();
        let next_fd_number = AtomicUsize::new(0);

        let mut new_fds = kvec();
        for task in list.iter() {
            new_fds.push(task.fork());
        }

        TaskFdTable {
            fds: Mutex::new(new_fds),
            next_fd_number,
        }
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
    pub fn fork(&self) -> TaskFd {
        match self {
            TaskFd::File {
                descriptor,
                inode,
                offset,
            } => TaskFd::File {
                descriptor: *descriptor,
                inode: inode.clone(),
                offset: AtomicUsize::new(offset.load(SeqCst)),
            },
        }
    }

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
