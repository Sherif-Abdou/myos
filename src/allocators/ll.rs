use core::{
    alloc::{AllocError, Allocator, Layout},
    num::NonZero,
    ptr::NonNull,
};

use crate::{
    allocators::align_up,
    memory::{PAGE_ALLOCATOR, PAGE_SIZE, Pfn, page_from_pfn},
    utils::SpinLock,
};

struct LLHole {
    size: usize,
    next: *mut LLHole,
}

struct LLCursor {
    head: NonNull<*mut LLHole>,
    previous: *mut LLHole,
    hole: *mut LLHole,
}

impl LLCursor {
    unsafe fn get(&mut self) -> Option<&LLHole> {
        unsafe { self.hole.as_ref() }
    }

    fn get_raw(&mut self) -> *mut LLHole {
        self.hole
    }

    unsafe fn get_next(&mut self) -> *mut LLHole {
        unsafe { (*self.hole).next }
    }

    unsafe fn get_previous(&mut self) -> *mut LLHole {
        self.previous
    }

    unsafe fn remove(&mut self) {
        if self.hole.is_null() {
            return;
        }
        if self.previous.is_null() {
            unsafe {
                self.head.write((*self.hole).next);
            }
        } else {
            unsafe { (*self.previous).next = (*self.hole).next }
        }
        unsafe {
            self.hole = (*self.hole).next;
        }
    }

    unsafe fn remove_next(&mut self) {
        if self.hole.is_null() || unsafe { self.get_next().is_null() } {
            return;
        }
        unsafe {
            let next = self.get_next();
            (*self.hole).next = (*next).next;
        }
    }

    // Inserts `ptr` right after the cursor.
    unsafe fn insert(&mut self, ptr: *mut LLHole) {
        assert!(!ptr.is_null());

        if !self.hole.is_null() {
            unsafe {
                (*ptr).next = (*self.hole).next;
                (*self.hole).next = ptr;
            }
        } else {
            unsafe {
                (*ptr).next = core::ptr::null_mut();
                self.head.write(ptr);
                self.hole = ptr;
            }
        }
    }

    // Inserts `ptr` right after the cursor.
    unsafe fn insert_before(&mut self, ptr: *mut LLHole) {
        assert!(!ptr.is_null());

        unsafe {
            (*ptr).next = self.hole;
            if !self.previous.is_null() {
                (*self.previous).next = ptr;
            } else {
                self.head.write(ptr);
            }
        }
    }

    unsafe fn next(&mut self) {
        assert!(!self.hole.is_null());
        self.previous = self.hole;
        unsafe { self.hole = (*self.hole).next };
    }

    fn is_null(&self) -> bool {
        self.hole.is_null()
    }
}

struct LLHeader {
    region_start: *mut u8,
    region_end: *mut u8,
}

struct LLStatistics {
    bytes_allocated: usize,
}

impl LLStatistics {
    pub const fn new() -> Self {
        Self { bytes_allocated: 0 }
    }
}

pub struct LLAllocator {
    first_hole: *mut LLHole,
    stats: LLStatistics,
}

unsafe impl Send for LLAllocator {}

impl LLAllocator {
    pub const fn new() -> Self {
        Self {
            first_hole: core::ptr::null_mut(),
            stats: LLStatistics::new(),
        }
    }

    fn cursor(&mut self) -> LLCursor {
        LLCursor {
            head: NonNull::new(&raw mut self.first_hole).unwrap(),
            previous: core::ptr::null_mut(),
            hole: self.first_hole,
        }
    }

    fn claim_more_memory(&mut self, size_hint: usize) {
        let pages = (size_hint.div_ceil(PAGE_SIZE) + 1).max(8);
        let block = PAGE_ALLOCATOR.lock().reserve_pages(pages).unwrap();

        for pfn in block.number()..(block.number() + pages) {
            let page = page_from_pfn(Pfn::from(pfn));
            page.inc_refcount();
            page.spin_lock().set_kernel();
        }

        let ll_hole: *mut LLHole = block.as_kernel_ptr().cast();
        let ll_hole_addr = ll_hole.addr();
        unsafe {
            (*ll_hole).size = pages * PAGE_SIZE;
            (*ll_hole).next = core::ptr::null_mut();
        }
        let mut cursor = self.cursor();

        unsafe {
            if cursor.is_null() || cursor.get_raw().addr() > ll_hole_addr {
                (*ll_hole).next = self.first_hole;
                self.first_hole = ll_hole;
            } else {
                // Find furthest hole before target address.
                while !cursor.is_null()
                    && !cursor.get_next().is_null()
                    && cursor.get_next().addr() < ll_hole_addr
                {
                    cursor.next();
                }
                // Insert hole.
                cursor.insert(ll_hole);
                // Move cursor to inserted hole.
                cursor.next();
            }
        }
    }
}

impl SpinLock<LLAllocator> {
    pub fn bytes_allocated(&self) -> usize {
        self.lock().stats.bytes_allocated
    }
}

unsafe impl Allocator for SpinLock<LLAllocator> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let necessary_alignment = align_of::<LLHole>().max(layout.align());
        let hole_alignment = align_of::<LLHole>();
        let mut locked_self = self.lock();

        let mut cursor = locked_self.cursor();
        while !cursor.is_null() {
            let hole_addr = cursor.get_raw() as usize;
            let hole_size = unsafe { cursor.get().unwrap().size };

            let start_hole_addr = hole_addr;
            let ptr_addr = align_up(hole_addr + size_of::<LLHeader>(), necessary_alignment);
            let ptr_end = align_up(ptr_addr + layout.size(), hole_alignment);
            let end_hole_addr = hole_addr + hole_size;

            let necessary_size = ptr_end - start_hole_addr;
            if hole_size >= necessary_size {
                let header_addr = ptr_addr - size_of::<LLHeader>();
                let header = header_addr as *mut LLHeader;
                let end = end_hole_addr.min(ptr_end + 2 * size_of::<LLHole>());

                // We replaced the current hole with an allocation.
                unsafe {
                    cursor.remove();
                }

                unsafe {
                    (*header).region_start = start_hole_addr as _;
                    (*header).region_end = end as _;
                }

                // There's enough room for an extra hole, add one
                if end_hole_addr >= ptr_end + 2 * size_of::<LLHole>() {
                    // Decrease the allocation size to just the area needed
                    unsafe {
                        (*header).region_end = ptr_end as _;
                    }
                    let ll_hole = ptr_end as *mut LLHole;
                    unsafe {
                        (*ll_hole).size = end_hole_addr - ptr_end;
                        cursor.insert_before(ll_hole);
                    }
                }

                locked_self.stats.bytes_allocated = locked_self
                    .stats
                    .bytes_allocated
                    .saturating_add(layout.size());

                return Ok(NonNull::slice_from_raw_parts(
                    NonNull::new(ptr_addr as *mut u8).ok_or(AllocError)?,
                    layout.size(),
                ));
            }

            unsafe {
                cursor.next();
            }
        }

        locked_self.claim_more_memory(layout.size());
        drop(locked_self);
        self.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let mut locked_self = self.lock();

        let ll_header = unsafe { ptr.byte_sub(size_of::<LLHeader>()) }.cast::<LLHeader>();
        let mut cursor = locked_self.cursor();

        let hole_size = unsafe {
            (ll_header.read().region_end as usize) - (ll_header.read().region_start as usize)
        };

        // Replace the allocation header with a hole.
        let ll_hole: NonNull<LLHole> = ll_header
            .cast()
            .with_addr(unsafe { NonZero::new_unchecked(ll_header.read().region_start.addr()) });
        unsafe {
            (*ll_hole.as_ptr()).size = hole_size;
            (*ll_hole.as_ptr()).next = core::ptr::null_mut();
        }

        let ll_hole_addr = ll_hole.addr().get();

        // Insert the hole into the linked list.
        unsafe {
            if cursor.get_raw().addr() > ll_hole_addr {
                (*ll_hole.as_ptr()).next = locked_self.first_hole;
                locked_self.first_hole = ll_hole.as_ptr();

                cursor = locked_self.cursor();
            } else {
                // Find furthest hole before target address.
                while !cursor.is_null()
                    && !cursor.get_next().is_null()
                    && cursor.get_next().addr() < ll_hole_addr
                {
                    cursor.next();
                }
                // Insert hole.
                cursor.insert(ll_hole.as_ptr());
                // Move cursor to inserted hole.
                cursor.next();
            }
        }

        unsafe {
            // If the current hole is immediately after previous hole,
            // coalesce into previous hole.
            while !cursor.is_null()
                && !cursor.get_previous().is_null()
                && cursor.get_previous().addr() + (*cursor.get_previous()).size
                    == cursor.get_raw().addr()
            {
                (*cursor.get_previous()).size += cursor.get().unwrap().size;
                cursor.remove();
            }

            // If next hole is immediately after current hole,
            // coalesce into current hole.
            while !cursor.is_null()
                && !cursor.get_next().is_null()
                && cursor.get_next().addr() == cursor.get_raw().addr() + (*cursor.get_raw()).size
            {
                (*cursor.get_raw()).size += (*cursor.get_next()).size;
                cursor.remove_next();
            }
        }

        locked_self.stats.bytes_allocated = locked_self
            .stats
            .bytes_allocated
            .saturating_sub(layout.size());
    }
}
