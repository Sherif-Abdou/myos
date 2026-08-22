use crate::{
    allocators::{KBox, KERNEL_ALLOCATOR, KVec},
    elf::Segment,
    impl_link,
    interrupts::ExceptionRegisters,
    subsystem::ArmPageTableRoot,
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
}

pub struct KernelSpaceTaskInfo {
    pub(crate) kernel_stack: KBox<KernelTaskStack>,
}

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
