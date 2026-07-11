mod ll;
use core::ptr::NonNull;

use alloc::{boxed::Box, vec::Vec};
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

use crate::utils::{CoreLock, LazyCoreLock, SpinLock};

unsafe extern "C" {
    pub unsafe static mut __heap_start: u8;
    pub unsafe static mut __heap_end: u8;
}

pub static BUMP_ALLOCATOR: LazyCoreLock<CoreLock<BumpAllocator>> = LazyCoreLock::new(|| {
    CoreLock::new(BumpAllocator::new(
        NonNull::new((&raw mut __heap_start) as _).unwrap(),
        NonNull::new((&raw mut __heap_end) as _).unwrap(),
    ))
});

pub static KERNEL_ALLOCATOR: SpinLock<LLAllocator> = SpinLock::new(LLAllocator::new());

pub type KBox<T> = Box<T, &'static SpinLock<LLAllocator>>;

pub fn kbox<T>(inner: T) -> KBox<T> {
    Box::new_in(inner, &KERNEL_ALLOCATOR)
}

pub type KVec<T> = Vec<T, &'static SpinLock<LLAllocator>>;

pub fn kvec<T>() -> KVec<T> {
    Vec::new_in(&KERNEL_ALLOCATOR)
}
