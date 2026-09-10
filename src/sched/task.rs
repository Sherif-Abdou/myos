use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering::SeqCst};

use crate::{
    allocators::{KBox, KERNEL_ALLOCATOR, KVec, kbox, kvec},
    elf::{ElfParser, ElfSource, Segment},
    impl_link, impl_rblink,
    interrupts::ExceptionRegisters,
    memory::{PAGE_SIZE, Pfn},
    sched::{
        LazyPageZeroedSource, Mutex, SCHEDULER, STACK_VIRTUAL_ADDR, WaitQueue,
        lazy_buffer::LazyPageBuffer, restore_regs_and_eret,
    },
    subsystem::{
        AnonPageMeta, ArmPageTableRoot, Inode, InodeOperations, PageFaultError, PageFaultType,
        VmaAllocatedArea,
    },
    utils::{
        Arc, List, ListArc, ListLinks, PhysAddr, RbLinks, RbTree, SpinLock, TreeArc, UniqueArc,
        with_core_critical_section,
    },
};

extern "C" fn thread_wrapper(f: extern "C" fn(*mut ()), arg: *mut ()) {
    f(arg);

    SCHEDULER.get().unwrap().end_task(0);

    let next_task = SCHEDULER.get().unwrap().next_task().unwrap();

    with_core_critical_section(|| {
        restore_regs_and_eret(&raw const next_task);
    });
}

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

    pub fn get(&mut self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn get_mut(&mut self) -> &mut [u8] {
        self.0.as_mut_slice()
    }

    pub fn phys_addr(&self) -> PhysAddr {
        PhysAddr::from(self.0.as_ptr())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

pub struct UserSpaceHeap(LazyPageBuffer<LazyPageZeroedSource>);

impl UserSpaceHeap {
    const DEFAULT_BASE_VMA: usize = 0x800_0000;

    pub fn new(base_vma: usize) -> Self {
        let vma_area = Arc::new(VmaAllocatedArea::new(base_vma, 0));
        let anon_vma = Arc::new(AnonPageMeta::new(None));

        anon_vma.insert_vma_area(TreeArc::try_from_arc(vma_area.clone()).unwrap());

        Self(LazyPageBuffer::new_zeroed(base_vma, base_vma))
    }

    pub fn modify_end(&mut self, offset: isize, table: &ArmPageTableRoot) -> usize {
        let original_end_address = self.0.end_address();
        let new_end_address = original_end_address.saturating_add_signed(offset);

        self.0.set_end_bound(new_end_address, table);

        new_end_address
    }

    pub fn fork(&self, parent_table: &ArmPageTableRoot, child_table: &ArmPageTableRoot) -> Self {
        Self(self.0.fork(parent_table, child_table))
    }

    fn release_pages(&self, table: &ArmPageTableRoot) {
        self.0.release_pages(table);
    }

    fn contains_vma(&self, vma: usize) -> bool {
        self.0.contains_vma(vma)
    }

    fn end_of_heap(&self) -> usize {
        self.0.end_address()
    }

    fn page_fault(
        &self,
        fault: PageFaultType,
        vma: usize,
        table: &SpinLock<ArmPageTableRoot>,
    ) -> Result<(), PageFaultError> {
        self.0.page_fault(fault, vma, table)
    }
}

pub struct UserSpaceProcess {
    pub(crate) page_table: SpinLock<ArmPageTableRoot>,
    pub(crate) user_stack: SpinLock<KBox<UserTaskStack>>,
    pub(crate) user_heap: SpinLock<KBox<UserSpaceHeap>>,
    pub(crate) kernel_stack: KBox<KernelTaskStack>,
    pub(crate) segments: SpinLock<KVec<LazyPageBuffer<Segment>>>,
    pub(crate) fds: TaskFdTable,
}

impl Drop for UserSpaceProcess {
    fn drop(&mut self) {
        let page_table = &self.page_table.lock();
        self.user_heap.lock().release_pages(page_table);

        for segment in self.segments.lock().iter() {
            segment.release_pages(page_table);
        }
    }
}

impl UserSpaceProcess {
    fn map_page_range(&self, vma: usize, pfn: Pfn, count: usize) {
        self.page_table
            .lock()
            .map_page_range(vma, pfn.phys_addr(), count);
    }

    fn fork(&self) -> UserSpaceProcess {
        let new_page_table = ArmPageTableRoot::create_user();

        let mut new_segments = kvec();

        let new_user_stack = UserTaskStack::clone_box(&self.user_stack.lock());
        let new_user_heap = self
            .user_heap
            .lock()
            .fork(&self.page_table.lock(), &new_page_table);
        let new_kernel_stack = KernelTaskStack::clone_box(&self.kernel_stack);
        let new_fds = self.fds.fork();

        let parent_page_table = self.page_table.lock();
        for segment in self.segments.lock().iter() {
            new_segments.push(segment.fork(&parent_page_table, &new_page_table));
        }
        drop(parent_page_table);

        new_page_table.map_page_range(
            STACK_VIRTUAL_ADDR,
            new_user_stack.phys_addr(),
            new_user_stack.len().div_ceil(4096),
        );

        UserSpaceProcess {
            page_table: SpinLock::new(new_page_table),
            user_stack: SpinLock::new(new_user_stack),
            user_heap: SpinLock::new(kbox(new_user_heap)),
            kernel_stack: new_kernel_stack,
            segments: SpinLock::new(new_segments),
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
    pub(crate) fn map_page_range(&self, vma: usize, pfn: Pfn, count: usize) {
        match self {
            Process::Kernel(_) => todo!(),
            Process::User(user_space_process) => {
                user_space_process.map_page_range(vma, pfn, count);
            }
        }
    }

    pub fn exec(&self, parser: &ElfParser, args: &[u8]) {
        let Process::User(user_process) = self else {
            return;
        };

        let num_segments = parser.num_segments();

        let mut segments = kvec();
        let page_table = ArmPageTableRoot::create_user();

        let mut user_stack = create_user_stack();
        if !args.is_empty() {
            let copy_start = user_stack.len() - args.len();
            user_stack.get_mut()[copy_start..].copy_from_slice(args);

            let argc = u64::from_le_bytes(args[..8].try_into().unwrap());
            for i in 0..argc {
                let argv = u64::from_le_bytes(
                    args[8 * (i as usize + 1)..8 * (i as usize + 2)]
                        .try_into()
                        .unwrap(),
                );

                let addr = argv as usize + copy_start + STACK_VIRTUAL_ADDR;
                user_stack.get_mut()
                    [copy_start + 8 * (i as usize + 1)..copy_start + 8 * (i as usize + 2)]
                    .copy_from_slice(&(addr).to_le_bytes());
            }
        }

        for index in 0..num_segments {
            let segment = parser.segment(index);
            if !segment.is_null() {
                let virt_addr = segment.virt_addr() & !0xfff;
                let pages = segment.mem_page_count();

                segments.push(LazyPageBuffer::new(
                    segment,
                    virt_addr,
                    virt_addr + pages * PAGE_SIZE,
                ));
            }
        }

        page_table.map_page_range(
            STACK_VIRTUAL_ADDR,
            user_stack.phys_addr(),
            user_stack.len().div_ceil(4096),
        );

        user_process
            .user_heap
            .lock()
            .release_pages(&user_process.page_table.lock());

        for segment in user_process.segments.lock().iter() {
            segment.release_pages(&user_process.page_table.lock());
        }

        *user_process.page_table.lock() = page_table;
        *user_process.segments.lock() = segments;
        *user_process.user_stack.lock() = user_stack;
        *user_process.user_heap.lock() = kbox(UserSpaceHeap::new(UserSpaceHeap::DEFAULT_BASE_VMA));
    }

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

static NEXT_PID_COUNTER: AtomicU32 = AtomicU32::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Runnable,
    Blocked,
    Done(i32),
}

#[repr(align(16))]
pub struct Task {
    pub(crate) pid: u32,
    pub(crate) state: SpinLock<TaskState>,
    pub(crate) completion_waiters: WaitQueue,
    pub(crate) registers: SpinLock<ExceptionRegisters>,
    pub(crate) run_queue_links: ListLinks,
    pub(crate) parent_links: ListLinks,
    pub(crate) process: Arc<Process>,
    pub(crate) children: SpinLock<List<Task, 1>>,
    pub(crate) parent_pid: u32,
}

impl Task {
    fn next_pid() -> u32 {
        NEXT_PID_COUNTER.fetch_add(1, SeqCst)
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn parent(&self) -> u32 {
        self.parent_pid
    }

    pub fn has_parent(&self) -> bool {
        self.parent_pid != 0
    }

    pub fn mark_running(&self) {
        *self.state.lock() = TaskState::Running;
    }

    pub fn is_done(&self) -> bool {
        matches!(*self.state.lock(), TaskState::Done(_))
    }

    pub fn is_blocked(&self) -> bool {
        matches!(*self.state.lock(), TaskState::Blocked)
    }

    pub fn with_state_locked<R, F: FnOnce(&mut TaskState) -> R>(&self, func: F) -> R {
        let mut state = self.state.lock();

        func(&mut state)
    }

    pub fn mark_runnable(&self) {
        *self.state.lock() = TaskState::Runnable;
    }

    pub fn mark_blocked(&self) {
        *self.state.lock() = TaskState::Blocked;
    }

    pub fn mark_done(&self, code: i32) {
        *self.state.lock() = TaskState::Done(code);
        self.completion_waiters.unblock_all();
    }

    pub fn fork_process(&self) -> u32 {
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

        let new_task = Arc::new(Task {
            pid: Self::next_pid(),
            state: SpinLock::new(TaskState::Runnable),
            completion_waiters: WaitQueue::new(),
            registers: SpinLock::new(new_registers),
            run_queue_links: ListLinks::new(),
            parent_links: ListLinks::new(),
            process: new_process,
            children: SpinLock::new(List::new()),
            parent_pid: self.pid(),
        });

        let Ok(scheduler_link) = ListArc::try_from_arc(new_task.clone()) else {
            panic!("Could not get scheduler link of forked task.");
        };

        let Ok(parent_link) = ListArc::try_from_arc(new_task.clone()) else {
            panic!("Could not get parent link of forked task.");
        };

        self.children.lock().push_back(parent_link);

        let pid = new_task.pid;

        SCHEDULER.get().unwrap().append_task(scheduler_link);

        pid
    }

    pub fn exec(&self, elf: Arc<dyn ElfSource + Send + Sync>, args: &[u8]) -> ExceptionRegisters {
        assert!(args.len() < STACK_SIZE);

        let parser = ElfParser::new(elf);

        self.process.exec(&parser, args);

        let mut registers = self.registers.lock();
        registers.elr = parser.entry_vma() as u64;
        registers.sp_el0 = STACK_VIRTUAL_ADDR as u64 + STACK_SIZE as u64 - args.len() as u64;
        if !args.is_empty() {
            registers.gprs[0] = u64::from_le_bytes(args[0..8].try_into().unwrap());
        } else {
            registers.gprs[0] = 0;
        }
        registers.gprs[1] = STACK_VIRTUAL_ADDR as u64 + STACK_SIZE as u64 - args.len() as u64 + 8;

        assert!(
            registers.gprs[31].is_multiple_of(16),
            "Stack ptr is not aligned"
        );
        registers.spsr = 0b0000;
        let copied = registers.clone();
        drop(registers);
        copied
    }

    #[allow(clippy::fn_to_numeric_cast, function_casts_as_integer)]
    pub fn kernel_task_from_fn(f: fn(*mut ()), arg: *mut ()) -> UniqueArc<Task> {
        let task = UniqueArc::new(Task {
            pid: Self::next_pid(),
            state: SpinLock::new(TaskState::Runnable),
            completion_waiters: WaitQueue::new(),
            registers: SpinLock::new(ExceptionRegisters::default()),
            run_queue_links: ListLinks::new(),
            parent_links: ListLinks::new(),
            process: Arc::new(Process::new_kernel(create_kernel_stack())),
            children: SpinLock::new(List::new()),
            parent_pid: 0,
        });

        let mut registers = task.registers.lock();
        registers.elr = (thread_wrapper) as u64;
        registers.gprs[0] = (f as usize) as u64;
        registers.gprs[1] = arg as u64;
        registers.gprs[31] = task.stack_top();
        assert!(
            registers.gprs[31].is_multiple_of(16),
            "Stack ptr is not aligned"
        );
        registers.spsr = 0b0101;
        drop(registers);

        task
    }

    pub fn load_program(elf: Arc<dyn ElfSource + Send + Sync>) -> UniqueArc<Task> {
        let parser = ElfParser::new(elf);

        let num_segments = parser.num_segments();

        let mut segments = kvec();
        let page_table = ArmPageTableRoot::create_user();

        let user_stack = create_user_stack();

        for index in 0..num_segments {
            let segment = parser.segment(index);
            if !segment.is_null() {
                let virt_addr = segment.virt_addr() & !0xfff;
                let pages = segment.mem_page_count();

                segments.push(LazyPageBuffer::new(
                    segment,
                    virt_addr,
                    virt_addr + pages * PAGE_SIZE,
                ));
            }
        }

        page_table.map_page_range(
            STACK_VIRTUAL_ADDR,
            user_stack.phys_addr(),
            user_stack.len().div_ceil(4096),
        );

        let userspace = UserSpaceProcess {
            page_table: SpinLock::new(page_table),
            segments: SpinLock::new(segments),
            user_stack: SpinLock::new(user_stack),
            user_heap: SpinLock::new(kbox(UserSpaceHeap::new(UserSpaceHeap::DEFAULT_BASE_VMA))),
            kernel_stack: create_kernel_stack(),
            fds: TaskFdTable::new(),
        };

        let task = UniqueArc::new(Task {
            pid: Self::next_pid(),
            state: SpinLock::new(TaskState::Runnable),
            completion_waiters: WaitQueue::new(),
            registers: SpinLock::new(ExceptionRegisters::default()),
            run_queue_links: ListLinks::new(),
            parent_links: ListLinks::new(),
            process: Arc::new(Process::User(userspace)),
            children: SpinLock::new(List::new()),
            parent_pid: 0,
        });

        let mut registers = task.registers.lock();
        registers.elr = parser.entry_vma() as u64;
        registers.gprs[31] = task.kernel_stack_top();
        registers.sp_el0 = STACK_VIRTUAL_ADDR as u64 + STACK_SIZE as u64;

        assert!(
            registers.gprs[31].is_multiple_of(16),
            "Stack ptr is not aligned"
        );
        registers.spsr = 0b0000;
        drop(registers);

        task
    }

    pub fn bind_pages(&self) {
        match *self.process {
            Process::Kernel(_) => {}
            Process::User(ref user_space_process) => {
                user_space_process.page_table.lock().bind_user();
            }
        }
    }

    pub fn stack_top(&self) -> u64 {
        match *self.process {
            Process::Kernel(ref kernel_space_task_info) => {
                kernel_space_task_info.kernel_stack.0.as_ptr().addr() as u64 + STACK_SIZE as u64
            }
            Process::User(ref user_space_task_info) => {
                user_space_task_info.user_stack.lock().0.as_ptr().addr() as u64 + STACK_SIZE as u64
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

    pub fn find_child(&self, pid: u32) -> Option<Arc<Task>> {
        let children = self.children.lock();
        let mut cursor = children.cursor();

        while let Some(child) = cursor.get_arc() {
            if child.pid() == pid {
                return Some(child);
            } else {
                let _ = cursor.next();
            }
        }

        None
    }

    pub fn kill_children(&self) {
        let mut children = self.children.lock();
        let mut cursor = children.cursor_mut();

        while let Some(child) = cursor.get() {
            child.kill_children();
            cursor.remove();
        }
    }

    pub fn handle_page_fault(
        &self,
        fault_type: PageFaultType,
        vma: usize,
    ) -> Result<(), PageFaultError> {
        if let Process::User(ref user_process) = *self.process {
            let heap = user_process.user_heap.lock();

            if heap.contains_vma(vma) {
                return heap.page_fault(fault_type, vma, &user_process.page_table);
            }
            drop(heap);
            for segment in user_process.segments.lock().iter() {
                if segment.contains_vma(vma) {
                    return segment.page_fault(fault_type, vma, &user_process.page_table);
                }
            }
        }

        Err(PageFaultError::Unhandled)
    }

    pub fn offset_heap(&self, offset: isize) -> Result<usize, ()> {
        if let Process::User(ref user_process) = *self.process {
            let mut heap = user_process.user_heap.lock();
            let page_table = user_process.page_table.lock();

            return Ok(heap.modify_end(offset, &page_table));
        }

        Ok(0)
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

impl_link!(Task, 0 => run_queue_links, 1 => parent_links);

pub struct TaskFdTable {
    fds: Mutex<RbTree<TaskFd>>,
    next_fd_number: AtomicUsize,
}

impl TaskFdTable {
    pub fn new() -> Self {
        Self {
            fds: Mutex::new(RbTree::new()),
            next_fd_number: AtomicUsize::new(11),
        }
    }

    pub fn find_inode(&self, descriptor: usize) -> Option<Arc<Inode>> {
        self.fds
            .lock()
            .find(descriptor)
            .and_then(|fd| fd.inode().cloned())
    }

    pub fn add_file_fd(&self, inode: Arc<Inode>) -> usize {
        let mut fds = self.fds.lock();
        let descriptor = self.next_fd_number.fetch_add(1, SeqCst);

        fds.insert(
            UniqueArc::new(TaskFd {
                descriptor,
                inner: Arc::new(TaskFdInner::File {
                    inode,
                    offset: AtomicUsize::new(0),
                }),
                links: RbLinks::new(),
            })
            .into(),
        );

        descriptor
    }

    pub fn add_anon_file_fd(&self, inode_ops: Arc<dyn InodeOperations>) -> usize {
        let mut fds = self.fds.lock();
        let descriptor = self.next_fd_number.fetch_add(1, SeqCst);

        fds.insert(
            UniqueArc::new(TaskFd {
                descriptor,
                inner: Arc::new(TaskFdInner::AnonFile {
                    inode_ops,
                    offset: AtomicUsize::new(0),
                }),
                links: RbLinks::new(),
            })
            .into(),
        );

        descriptor
    }

    pub fn dup_fd(&self, dst: u32, src: u32) -> Result<(), ()> {
        let mut fds = self.fds.lock();

        let Some(src) = fds.find(src as usize) else {
            return Err(());
        };
        fds.remove_key(dst as usize);
        fds.insert(UniqueArc::new(src.dup(dst as usize)).into());

        Ok(())
    }

    pub fn read(&self, descriptor: usize, buf: &mut [u8]) -> isize {
        let fds = self.fds.lock();
        let Some(fd) = fds.find(descriptor) else {
            return -1;
        };

        fd.read(buf)
    }

    pub fn write(&self, descriptor: usize, buf: &[u8]) -> isize {
        let fds = self.fds.lock();
        let Some(fd) = fds.find(descriptor) else {
            return -1;
        };

        fd.write(buf)
    }

    pub fn close(&self, descriptor: usize) {
        let mut fds = self.fds.lock();

        fds.remove_key(descriptor);
    }

    pub fn fork(&self) -> TaskFdTable {
        let list = self.fds.lock();
        let next_fd_number = AtomicUsize::new(self.next_fd_number.load(SeqCst));

        let mut new_fds = RbTree::new();
        for task in list.cursor() {
            new_fds.insert(UniqueArc::new(task.fork()).into());
        }

        TaskFdTable {
            fds: Mutex::new(new_fds),
            next_fd_number,
        }
    }
}

struct TaskFd {
    descriptor: usize,
    inner: Arc<TaskFdInner>,
    links: RbLinks,
}

impl_rblink!(TaskFd, let descriptor: usize = { 0 => links });

enum TaskFdInner {
    File {
        inode: Arc<Inode>,
        offset: AtomicUsize,
    },
    AnonFile {
        inode_ops: Arc<dyn InodeOperations>,
        offset: AtomicUsize,
    },
}

impl TaskFd {
    pub fn inode(&self) -> Option<&Arc<Inode>> {
        match &*self.inner {
            TaskFdInner::File { inode, offset: _ } => Some(inode),
            _ => None,
        }
    }

    pub fn dup(&self, descriptor: usize) -> Self {
        Self {
            descriptor,
            inner: self.inner.clone(),
            links: RbLinks::new(),
        }
    }

    pub fn fork(&self) -> TaskFd {
        match &*self.inner {
            TaskFdInner::File { inode, offset } => TaskFd {
                descriptor: self.descriptor,
                inner: Arc::new(TaskFdInner::File {
                    inode: inode.clone(),
                    offset: AtomicUsize::new(offset.load(SeqCst)),
                }),
                links: RbLinks::new(),
            },
            TaskFdInner::AnonFile { inode_ops, offset } => TaskFd {
                descriptor: self.descriptor,
                inner: Arc::new(TaskFdInner::AnonFile {
                    inode_ops: inode_ops.clone(),
                    offset: AtomicUsize::new(offset.load(SeqCst)),
                }),
                links: RbLinks::new(),
            },
        }
    }

    pub fn descriptor(&self) -> usize {
        self.descriptor
    }

    pub fn read(&self, buf: &mut [u8]) -> isize {
        match &*self.inner {
            TaskFdInner::File { inode, offset } => {
                let local_offset = offset.load(SeqCst);
                if let Ok(read) = Inode::read(inode, local_offset as u64, buf) {
                    offset.fetch_add(read, SeqCst);

                    read as isize
                } else {
                    -1
                }
            }
            TaskFdInner::AnonFile { inode_ops, offset } => {
                let local_offset = offset.load(SeqCst);
                if let Ok(read) = inode_ops.read(local_offset as u64, buf) {
                    offset.fetch_add(read, SeqCst);

                    read as isize
                } else {
                    -1
                }
            }
        }
    }

    pub fn write(&self, buf: &[u8]) -> isize {
        match &*self.inner {
            TaskFdInner::File { inode, offset } => {
                let local_offset = offset.load(SeqCst);
                if let Ok(len) = inode.write(local_offset as u64, buf) {
                    offset.fetch_add(len, SeqCst);

                    len as isize
                } else {
                    -1
                }
            }
            TaskFdInner::AnonFile { inode_ops, offset } => {
                let local_offset = offset.load(SeqCst);
                if let Ok(len) = inode_ops.write(local_offset as u64, buf) {
                    offset.fetch_add(len, SeqCst);

                    len as isize
                } else {
                    -1
                }
            }
        }
    }
}
