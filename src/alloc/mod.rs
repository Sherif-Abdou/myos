use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::{NonNull, null_mut},
};

use crate::utils::CoreLock;

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

    // Inserts `ptr` right after the cursor.
    unsafe fn insert(&mut self, ptr: *mut LLHole) {
        assert!(!ptr.is_null());

        unsafe {
            (*ptr).next = (*self.hole).next;
            (*self.hole).next = ptr;
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

pub struct LLAllocator {
    first_hole: *mut LLHole,
}

impl LLAllocator {
    pub fn new(start: *mut u8, size: usize) -> Self {
        let first_hole = start as *mut LLHole;
        unsafe {
            (*first_hole).size = size;
            (*first_hole).next = core::ptr::null_mut();
        }

        Self {
            first_hole,
        }
    }

    fn cursor(&mut self) -> LLCursor {
        LLCursor {
            head: NonNull::new(&raw mut self.first_hole).unwrap(),
            previous: core::ptr::null_mut(),
            hole: self.first_hole,
        }
    }
}

const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

unsafe impl GlobalAlloc for CoreLock<LLAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let necessary_alignment = align_of::<LLHole>().max(layout.align());
        let hole_alignment = align_of::<LLHole>();
        let mut locked = self.lock();

        let mut cursor = locked.cursor();
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
                        (*ll_hole).size = end_hole_addr - ptr_end - size_of::<LLHole>();
                        cursor.insert(ll_hole);
                    }
                }

                // We replaced the current hole with an allocation.
                unsafe {
                    cursor.remove();
                }

                return ptr_addr as *mut u8;
            }

            unsafe {
                cursor.next();
            }
        }

        null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        let mut locked = self.lock();

        let ll_header = unsafe { ptr.byte_sub(size_of::<LLHeader>()) } as *mut LLHeader;
        let mut cursor = locked.cursor();

        let hole_size = unsafe {
            ((*ll_header).region_end as usize) - ((*ll_header).region_start as usize)
        };

        // Replace the allocation header with a hole.
        let ll_hole = ll_header as *mut LLHole;
        unsafe {
            (*ll_hole).size = hole_size;
            (*ll_hole).next = core::ptr::null_mut();
        }

        let ll_hole_addr = ll_hole as usize;

        // Insert the hole into the linked list.
        unsafe {
            // Find furthest hole before target address.
            while !cursor.is_null() && cursor.get_next().addr() < ll_hole_addr {
                cursor.next();
            }
            // Insert hole.
            cursor.insert(ll_hole);
            // Move cursor to inserted hole.
            cursor.next();
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
        }
    }
}
