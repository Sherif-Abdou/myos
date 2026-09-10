use crate::{
    memory::{PAGE_ALLOCATOR, PAGE_SIZE, page_from_pfn},
    printk,
    sched::cpu_current_task,
    subsystem::{AnonPageMeta, ArmPageTableRoot, PageFaultError, PageFaultType, VmaAllocatedArea},
    utils::{Arc, SpinLock, TreeArc},
};

pub trait LazyPageBufferSource {
    fn read_page(&self, offset: usize, buf: &mut [u8]);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LazyPageUninitSource;

impl LazyPageBufferSource for LazyPageUninitSource {
    fn read_page(&self, offset: usize, buf: &mut [u8]) {
        let _ = offset;
        let _ = buf;
    }
}

impl LazyPageBufferSource for LazyPageZeroedSource {
    fn read_page(&self, offset: usize, buf: &mut [u8]) {
        let _ = offset;

        buf.fill(0);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LazyPageZeroedSource;

#[derive(Copy, Debug, Clone, PartialEq, Eq)]
pub struct LazyVmaBounds(usize, usize);

impl LazyVmaBounds {
    pub fn start(&self) -> usize {
        self.0
    }

    pub fn end(&self) -> usize {
        self.1
    }

    pub fn set_start(&mut self, start: usize) {
        self.0 = start;
    }

    pub fn set_end(&mut self, end: usize) {
        self.1 = end;
    }

    pub fn contains(&self, vma: usize) -> bool {
        self.start() <= vma && vma < self.end()
    }
}

pub struct LazyPageBuffer<S: LazyPageBufferSource> {
    vma_area: Arc<VmaAllocatedArea>,
    anon_vma: Arc<AnonPageMeta>,
    vma_bounds: SpinLock<LazyVmaBounds>,
    source: S,
}

impl LazyPageBuffer<LazyPageUninitSource> {
    pub fn new_uninit(start_address: usize, end_address: usize) -> Self {
        Self::new(LazyPageUninitSource, start_address, end_address)
    }
}

impl LazyPageBuffer<LazyPageZeroedSource> {
    pub fn new_zeroed(start_address: usize, end_address: usize) -> Self {
        Self::new(LazyPageZeroedSource, start_address, end_address)
    }
}

impl<S: LazyPageBufferSource + Clone> Clone for LazyPageBuffer<S> {
    fn clone(&self) -> Self {
        Self {
            vma_area: self.vma_area.clone(),
            anon_vma: self.anon_vma.clone(),
            vma_bounds: SpinLock::new(*self.vma_bounds.lock()),
            source: self.source.clone(),
        }
    }
}

impl<S: LazyPageBufferSource + Clone> LazyPageBuffer<S> {
    pub fn fork(&self, parent_table: &ArmPageTableRoot, child_table: &ArmPageTableRoot) -> Self {
        let base_vma = self.vma_area.vma();
        let pfn_count = self.vma_area.pfn_count();
        let end_vma = base_vma + pfn_count * PAGE_SIZE;

        let vma_area = Arc::new(VmaAllocatedArea::new(base_vma, pfn_count));
        let anon_vma = Arc::new(AnonPageMeta::new(Some(self.anon_vma.clone())));

        anon_vma.insert_vma_area(TreeArc::try_from_arc(vma_area.clone()).unwrap());

        parent_table.for_each_valid_page(|page_vma, page| {
            if base_vma <= page_vma && page_vma < end_vma {
                page.set_readonly(true);

                // Unwrap is safe because the page has to be valid.
                let parent_pfn = page.get_page().unwrap();
                let parent_page = page_from_pfn(parent_pfn);
                parent_page.inc_refcount();

                // let child_pfn = PAGE_ALLOCATOR.lock().reserve_pages(1).unwrap();
                // let child_page = page_from_pfn(child_pfn);
                // child_page.inc_refcount();
                // child_page.spin_lock().set_anon(anon_vma.clone());
                //
                // copy_pfn(child_pfn, parent_pfn);

                child_table.map_page_range_ro(page_vma, parent_pfn.phys_addr(), 1);
            }
        });

        Self {
            vma_area,
            anon_vma,
            source: self.source.clone(),
            vma_bounds: SpinLock::new(*self.vma_bounds.lock()),
        }
    }
}

impl<S: LazyPageBufferSource> LazyPageBuffer<S> {
    pub fn inner_ref(&self) -> &S {
        &self.source
    }

    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.source
    }

    pub fn new(source: S, start_address: usize, end_address: usize) -> Self {
        let page_count = end_address.saturating_sub(start_address) / PAGE_SIZE;
        let vma_area = Arc::new(VmaAllocatedArea::new(start_address, page_count));
        let anon_vma = Arc::new(AnonPageMeta::new(None));

        anon_vma.insert_vma_area(TreeArc::try_from_arc(vma_area.clone()).unwrap());

        Self {
            vma_area,
            anon_vma,
            source,
            vma_bounds: SpinLock::new(LazyVmaBounds(start_address, end_address)),
        }
    }

    pub fn start_address(&self) -> usize {
        self.vma_bounds.lock().start()
    }

    pub fn contains_vma(&self, vma: usize) -> bool {
        self.vma_bounds.lock().contains(vma)
    }

    pub fn end_address(&self) -> usize {
        self.vma_bounds.lock().end()
    }

    pub fn set_end_bound(&mut self, end_address: usize, table: &ArmPageTableRoot) {
        let old_end_address = self.vma_bounds.lock().end();

        let end_page_before = (old_end_address.div_ceil(PAGE_SIZE)) as isize;
        let end_page_after = (end_address.div_ceil(PAGE_SIZE)) as isize;
        let page_diff = end_page_after - end_page_before;
        self.vma_area.modify_pfn_count(page_diff);

        if end_address < self.vma_bounds.lock().end() {
            table.for_each_valid_page(|vma, mut page| {
                if end_address <= vma && vma < old_end_address {
                    let page_meta = page_from_pfn(page.get_page().unwrap());
                    page_meta.dec_refcount();
                    page.set_valid(false);
                }
            });
        }

        self.vma_bounds.lock().set_end(end_address);
    }

    pub fn page_fault(
        &self,
        fault: PageFaultType,
        vma: usize,
        table: &SpinLock<ArmPageTableRoot>,
    ) -> Result<(), PageFaultError> {
        match fault {
            PageFaultType::Translation => {
                let byte_offset = vma - self.vma_area.vma();
                let page_offset = byte_offset / PAGE_SIZE;
                if page_offset >= self.vma_area.pfn_count() {
                    printk!("Out of range page\n");
                    return Err(PageFaultError::Unhandled);
                }

                let pfn = PAGE_ALLOCATOR.lock().reserve_pages(1).unwrap();

                let page = page_from_pfn(pfn);
                page.inc_refcount();
                page.spin_lock().set_anon(self.anon_vma.clone());

                self.source
                    .read_page(page_offset * PAGE_SIZE, unsafe { pfn.as_mut_slice() });

                let task = cpu_current_task().unwrap();

                task.process.map_page_range(vma & !(PAGE_SIZE - 1), pfn, 1);
                Ok(())
            }
            PageFaultType::Access => Err(PageFaultError::Unhandled),
            PageFaultType::Permission => {
                let table = table.lock();
                let byte_offset = vma - self.vma_area.vma();
                let page_offset = byte_offset / PAGE_SIZE;
                if page_offset >= self.vma_area.pfn_count() {
                    printk!("Out of range page\n");
                    return Err(PageFaultError::Unhandled);
                }

                let page_start_vma = vma & !(PAGE_SIZE - 1);

                table.for_each_valid_page(|vma, descriptor| {
                    if vma == page_start_vma {
                        let current_pfn = descriptor.get_page().unwrap();
                        let new_pfn = PAGE_ALLOCATOR.lock().reserve_pages(1).unwrap();

                        let new_page = page_from_pfn(new_pfn);
                        new_page.inc_refcount();
                        new_page.spin_lock().set_anon(self.anon_vma.clone());

                        let current_page = page_from_pfn(current_pfn);

                        unsafe { new_pfn.as_mut_slice() }
                            .copy_from_slice(unsafe { current_pfn.as_slice() });

                        current_page.dec_refcount();

                        descriptor.map_page(new_pfn);
                        descriptor.set_readonly(false);
                    }
                });

                Ok(())
            }
        }
    }

    pub fn release_pages(&self, table: &ArmPageTableRoot) {
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
}
