mod ll;
use core::ptr::NonNull;

pub use ll::LLAllocator;

struct FakeAllocator;

unsafe impl core::alloc::GlobalAlloc for FakeAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        panic!("Global Allocator Not supported");
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        panic!("Global Allocator Not supported");
    }
}

#[global_allocator]
static ALLOCATOR: FakeAllocator = FakeAllocator;

pub(crate) const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

mod bump;
pub use bump::BumpAllocator;

use crate::utils::{CoreLock, LazyCoreLock};

unsafe extern "C" {
    pub unsafe static mut __heap_start: u8;
    pub unsafe static mut __heap_end: u8;
}

pub static BUMP_ALLOCATOR: LazyCoreLock<CoreLock<BumpAllocator>> = LazyCoreLock::new(|| {
    CoreLock::new(BumpAllocator::new(
        NonNull::new(unsafe { (&raw mut __heap_start) as _ }).unwrap(),
        NonNull::new(unsafe { (&raw mut __heap_end) as _ }).unwrap(),
    ))
});
