use core::ptr;

use crate::{dtb::FdtNode, early_printk, utils::SpinLock};

/// Page number
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct Pfn(usize);

impl Pfn {
    pub const fn from_virt_addr(virt: usize) -> Self {
        Pfn((virt & 0xffffffff) >> PAGE_SHIFT)
    }

    pub fn from_ptr(ptr: *const ()) -> Self {
        let virt = ptr.addr();

        Self::from_virt_addr(virt)
    }

    pub const fn number(&self) -> usize {
        self.0
    }

    pub const fn as_ptr(&self) -> *mut u8 {
        ((self.0 * PAGE_SIZE) | 0xffffff80_00000000) as *mut u8
    }
}

impl From<usize> for Pfn {
    fn from(value: usize) -> Self {
        Pfn(value)
    }
}

impl From<Pfn> for usize {
    fn from(value: Pfn) -> Self {
        value.0
    }
}

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

const BITMASK_ELEMENT_SIZE: usize = core::mem::size_of::<u64>();

pub struct PageAllocator<'a> {
    bitmask: &'a mut [u64],
    offset: usize,
}

unsafe fn array_from_ptr(ptr: *mut u64, len: usize) -> &'static mut [u64] {
    let ptr = ptr::slice_from_raw_parts_mut(ptr, len);

    unsafe { ptr.as_mut().unwrap() }
}

impl PageAllocator<'_> {
    const fn bitmask_len_for_memory(size: usize) -> usize {
        let pages = size.div_ceil(PAGE_SIZE);

        pages.div_ceil(BITMASK_ELEMENT_SIZE)
    }

    pub fn is_free(&self, pfn: Pfn) -> bool {
        let local_pfn: usize = pfn.number() - self.offset;

        let index = local_pfn / BITMASK_ELEMENT_SIZE;
        let offset = local_pfn % BITMASK_ELEMENT_SIZE;

        self.bitmask[index] & (1 << offset) == 0
    }

    pub fn mark_used(&mut self, pfn: Pfn, count: usize) -> usize {
        let local_pfn = pfn.number() - self.offset;

        let effective_count =
            count.min((self.bitmask.len() * BITMASK_ELEMENT_SIZE).saturating_sub(local_pfn));
        for page in local_pfn..(local_pfn + effective_count) {
            let index = page / BITMASK_ELEMENT_SIZE;
            let offset = page % BITMASK_ELEMENT_SIZE;

            self.bitmask[index] |= 1 << offset;
        }
        effective_count
    }

    pub fn mark_free(&mut self, pfn: Pfn, count: usize) -> usize {
        let local_pfn = pfn.number() - self.offset;

        let effective_count =
            count.min((self.bitmask.len() * BITMASK_ELEMENT_SIZE).saturating_sub(local_pfn));
        for page in local_pfn..(local_pfn + effective_count) {
            let index = page / BITMASK_ELEMENT_SIZE;
            let offset = page % BITMASK_ELEMENT_SIZE;

            self.bitmask[index] &= !(1 << offset);
        }

        effective_count
    }

    pub fn reserve_pages(&mut self, count: usize) -> Option<Pfn> {
        let mut region_start = self.offset;
        let mut region_size = 0;

        for page in self.offset..(self.offset + self.bitmask.len() * BITMASK_ELEMENT_SIZE) {
            if self.is_free(page.into()) {
                region_size += 1;
            } else {
                region_start = page + 1;
                region_size = 0;
            }

            if region_size == count {
                let pfn = Pfn(region_start);
                self.mark_used(pfn, count);
                return Some(pfn);
            }
        }
        None
    }
}

unsafe extern "C" {
    unsafe static mut __kernel_start: usize;
    unsafe static mut __kernel_end: usize;
}

const DTB_LEN: usize = 0x10_0000;
static mut DUMMY: [u64; 1] = [0; 1];

pub static PAGE_ALLOCATOR: SpinLock<PageAllocator> = SpinLock::new(PageAllocator {
    #[allow(static_mut_refs)]
    bitmask: unsafe { &mut DUMMY },
    offset: 0,
});

pub fn init_allocator(memory: &FdtNode) {
    let reg_prop = memory
        .properties()
        .find(|prop| prop.name() == "reg")
        .unwrap();

    let memory_start = reg_prop.read_u64(0).unwrap() as usize;
    let memory_size = reg_prop.read_u64(1).unwrap() as usize;

    early_printk!(
        "Memory region: 0x{:x}-0x{:x}\n",
        memory_start,
        memory_start + memory_size
    );

    let kernel_end_ptr = (&raw mut __kernel_end);
    let next_page =
        unsafe { kernel_end_ptr.byte_offset(kernel_end_ptr.align_offset(PAGE_SIZE) as isize) };

    let bitmask_len = PageAllocator::bitmask_len_for_memory(memory_size);

    let bitmask = unsafe { array_from_ptr(next_page.cast(), bitmask_len) };
    let bitmask_pfn = Pfn::from_ptr(bitmask.as_ptr().cast());
    let bitmask_page_count = (bitmask.len() * BITMASK_ELEMENT_SIZE) / PAGE_SIZE;

    let mut allocator = PAGE_ALLOCATOR.lock();
    allocator.bitmask = bitmask;
    allocator.offset = memory_start / PAGE_SIZE;

    let kernel_image_pfn = Pfn((&raw const __kernel_start).addr() / PAGE_SIZE);
    let kernel_image_page_count =
        ((&raw const __kernel_end).addr() - (&raw const __kernel_start).addr()).div_ceil(PAGE_SIZE);

    // Reserve the DTB region
    allocator.mark_used(Pfn(memory_start), DTB_LEN / PAGE_SIZE);
    // Reserve the kernel image
    allocator.mark_used(kernel_image_pfn, kernel_image_page_count);
    // Reserve the page bitmask
    allocator.mark_used(bitmask_pfn, bitmask_page_count);
}
