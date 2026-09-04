use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering::SeqCst};

use crate::{
    allocators::{KBox, KERNEL_ALLOCATOR, KVec, kbox, kvec},
    elf::{ElfParser, ElfSource, Segment},
    impl_link,
    interrupts::ExceptionRegisters,
    memory::{PAGE_ALLOCATOR, PAGE_SIZE, Pfn, copy_pfn, page_from_pfn},
    printk,
    sched::{
        Mutex, SCHEDULER, STACK_VIRTUAL_ADDR, WaitQueue, cpu_current_task, restore_regs_and_eret,
    },
    subsystem::{
        AnonPageMeta, ArmPageTableRoot, Inode, PageFaultError, PageFaultType, VmaAllocatedArea,
    },
    utils::{
        Arc, List, ListArc, ListLinks, PhysAddr, SpinLock, TreeArc, UniqueArc,
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

pub struct UserSpaceHeap {
    vma_area: Arc<VmaAllocatedArea>,
    anon_vma: Arc<AnonPageMeta>,
}

impl UserSpaceHeap {
    const DEFAULT_BASE_VMA: usize = 0x800_0000;

    pub fn new(base_vma: usize) -> Self {
        let vma_area = Arc::new(VmaAllocatedArea::new(base_vma, 4));
        let anon_vma = Arc::new(AnonPageMeta::new(None));

        anon_vma.insert_vma_area(TreeArc::try_from_arc(vma_area.clone()).unwrap());

        Self { vma_area, anon_vma }
    }

    pub fn fork(&self, parent_table: &ArmPageTableRoot, child_table: &ArmPageTableRoot) -> Self {
        let base_vma = self.vma_area.vma();
        let pfn_count = self.vma_area.pfn_count();
        let end_vma = base_vma + pfn_count * PAGE_SIZE;

        let vma_area = Arc::new(VmaAllocatedArea::new(base_vma, pfn_count));
        let anon_vma = Arc::new(AnonPageMeta::new(Some(self.anon_vma.clone())));

        anon_vma.insert_vma_area(TreeArc::try_from_arc(vma_area.clone()).unwrap());

        parent_table.for_each_valid_page(|page_vma, page| {
            if base_vma <= page_vma && page_vma < end_vma {
                // Unwrap is safe because the page has to be valid.
                let parent_pfn = page.get_page().unwrap();

                let child_pfn = PAGE_ALLOCATOR.lock().reserve_pages(1).unwrap();
                let child_page = page_from_pfn(child_pfn);
                child_page.inc_refcount();
                child_page.spin_lock().set_anon(anon_vma.clone());

                copy_pfn(child_pfn, parent_pfn);

                child_table.map_page_range(page_vma, child_pfn.phys_addr(), 1);
            }
        });

        Self { vma_area, anon_vma }
    }

    fn release_pages(&self, table: &ArmPageTableRoot) {
        let base_vma = self.vma_area.vma();
        let pfn_count = self.vma_area.pfn_count();
        let end_vma = base_vma + pfn_count * PAGE_SIZE;

        table.for_each_valid_page(|page_vma, page| {
            if base_vma <= page_vma && page_vma < end_vma {
                // Unwrap is safe because the page has to be valid.
                let pfn = page.get_page().unwrap();

                page_from_pfn(pfn).dec_refcount();
            }
        });
    }

    fn contains_vma(&self, vma: usize) -> bool {
        self.vma_area.vma() <= vma && vma < self.end_of_heap()
    }

    fn end_of_heap(&self) -> usize {
        self.vma_area.vma() + self.vma_area.pfn_count() * PAGE_SIZE
    }

    pub fn modify_pfn_offset(&self, offset: isize) -> usize {
        self.vma_area.modify_pfn_count(offset)
    }

    fn page_fault(&self, fault_vma: usize) -> Result<(), PageFaultError> {
        let byte_offset = fault_vma - self.vma_area.vma();
        let page_offset = byte_offset / PAGE_SIZE;
        if page_offset >= self.vma_area.pfn_count() {
            printk!("Out of range page\n");
            return Err(PageFaultError::Unhandled);
        }
        printk!("Mapping vma {:x}\n", fault_vma);

        let pfn = PAGE_ALLOCATOR.lock().reserve_pages(1).unwrap();

        let page = page_from_pfn(pfn);
        page.inc_refcount();
        page.spin_lock().set_anon(self.anon_vma.clone());

        let task = cpu_current_task().unwrap();

        task.process
            .map_page_range(fault_vma & !(PAGE_SIZE - 1), pfn, 1);

        Ok(())
    }
}

pub struct UserSpaceProcess {
    pub(crate) page_table: SpinLock<ArmPageTableRoot>,
    pub(crate) user_stack: SpinLock<KBox<UserTaskStack>>,
    pub(crate) user_heap: SpinLock<KBox<UserSpaceHeap>>,
    pub(crate) kernel_stack: KBox<KernelTaskStack>,
    pub(crate) segments: SpinLock<KVec<Segment>>,
    pub(crate) fds: TaskFdTable,
}

impl Drop for UserSpaceProcess {
    fn drop(&mut self) {
        self.user_heap.lock().release_pages(&self.page_table.lock());
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

        let new_segments = self.segments.lock().clone();

        let new_user_stack = UserTaskStack::clone_box(&self.user_stack.lock());
        let new_user_heap = self
            .user_heap
            .lock()
            .fork(&self.page_table.lock(), &new_page_table);
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
    fn map_page_range(&self, vma: usize, pfn: Pfn, count: usize) {
        match self {
            Process::Kernel(_) => todo!(),
            Process::User(user_space_process) => {
                user_space_process.map_page_range(vma, pfn, count);
            }
        }
    }

    pub fn exec<S: ElfSource>(&self, parser: &ElfParser<S>, args: &[u8]) {
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
                let phys_addr = segment.loaded_phys_addr();
                let virt_addr = segment.virt_addr() & !0xfff;
                let pages = segment.mem_page_count();

                page_table.map_page_range(virt_addr, phys_addr, pages);

                segments.push(segment);
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

    pub fn exec(&self, elf: impl ElfSource, args: &[u8]) -> ExceptionRegisters {
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

    pub fn load_program(elf: impl ElfSource) -> UniqueArc<Task> {
        let parser = ElfParser::new(elf);

        let num_segments = parser.num_segments();

        let mut segments = kvec();
        let page_table = ArmPageTableRoot::create_user();

        let user_stack = create_user_stack();

        for index in 0..num_segments {
            let segment = parser.segment(index);
            if !segment.is_null() {
                let phys_addr = segment.loaded_phys_addr();
                let virt_addr = segment.virt_addr() & !0xfff;
                let pages = segment.mem_page_count();

                page_table.map_page_range(virt_addr, phys_addr, pages);

                segments.push(segment);
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
        fault_vma: usize,
    ) -> Result<(), PageFaultError> {
        printk!("Running task page fault handler\n");
        if let Process::User(ref user_process) = *self.process {
            let heap = user_process.user_heap.lock();

            if fault_type == PageFaultType::Translation {
                return heap.page_fault(fault_vma);
            }
        }

        Err(PageFaultError::Unhandled)
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
