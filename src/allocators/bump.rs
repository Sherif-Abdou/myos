use core::ptr::NonNull;

use alloc::alloc::{AllocError, Allocator};

use crate::utils::CoreLock;

pub struct BumpAllocator {
    current: NonNull<u8>,
    end: NonNull<u8>,
}

impl BumpAllocator {
    pub const fn new(start: NonNull<u8>, end: NonNull<u8>) -> Self {
        Self {
            current: start,
            end,
        }
    }
}

unsafe impl Allocator for CoreLock<BumpAllocator> {
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, alloc::alloc::AllocError> {
        let mut inner = self.lock();
        let start_addr = unsafe {
            inner
                .current
                .byte_offset(inner.current.align_offset(layout.align()) as isize)
        };
        let end_addr = unsafe { start_addr.byte_offset(layout.size() as isize) };

        if end_addr.addr() > inner.end.addr() {
            return Err(AllocError);
        }

        inner.current = end_addr;

        Ok(NonNull::slice_from_raw_parts(start_addr, layout.size()))
    }

    // Deallocation as a concept does not exist. The entire bump allocator must be cleared at once.
    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
    }
}
