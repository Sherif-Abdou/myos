use core::{
    arch::asm,
    cell::UnsafeCell,
    mem::{ManuallyDrop, MaybeUninit},
    sync::atomic::{AtomicU32, Ordering::SeqCst},
};

use crate::{
    impl_rblink,
    interrupts::daifset,
    utils::{Arc, RbLinks, RbTree, SpinLock},
};

// Page types: 3 bits
// 0 - Unused
// 1 - critical kernel
// 2 - userspace anon
// 3 - userspace fd-backed
#[repr(transparent)]
#[derive(Debug)]
pub struct PageFlags(AtomicU32);

impl PageFlags {
    pub const LOCKED: u32 = 1 << 0;
    pub const HEAD: u32 = 1 << 1;

    pub const PAGE_TYPE_MASK: u32 = 0xE0;
    pub const PAGE_TYPE_SHIFT: u32 = 5;

    fn is_head(&self) -> bool {
        self.0.load(SeqCst) & Self::HEAD != 0
    }

    /// Marks this page as a head page
    fn set_head(&self) {
        self.0.fetch_and(!Self::HEAD, SeqCst);
    }

    /// Unmarks this page as a head page
    fn clear_head(&self) {
        self.0.fetch_and(!Self::HEAD, SeqCst);
    }

    // Might set the cpu buses on fire
    fn try_lock(&self) -> Result<(), ()> {
        let current = self.0.load(SeqCst);
        if current & Self::LOCKED != 0 {
            return Err(());
        }
        let locked = current | Self::LOCKED;

        self.0
            .compare_exchange(current, locked, SeqCst, SeqCst)
            .map(|_| ())
            .map_err(|_| ())
    }

    fn spin_lock(&self) -> usize {
        let mut daif: usize = 0;

        unsafe {
            asm!("mrs {x}, daif", x = out(reg) daif);
        }

        daifset();
        while self.try_lock().is_err() {
            core::hint::spin_loop();
        }

        daif
    }

    fn page_type(&self) -> u32 {
        (self.0.load(SeqCst) & Self::PAGE_TYPE_MASK) >> Self::PAGE_TYPE_SHIFT
    }

    fn is_used(&self) -> bool {
        self.page_type() != 0
    }

    // Must lock before using
    fn set_page_type(&self, page_type: u32) {
        let before = self.0.load(SeqCst);

        self.0.store(
            (before & !Self::PAGE_TYPE_MASK) | (page_type << Self::PAGE_TYPE_SHIFT),
            SeqCst,
        );
    }

    fn unlock(&self) {
        self.0.fetch_xor(Self::LOCKED, SeqCst);
    }
}

/// Represents an anonymous area actively backed by pages.
pub struct VmaAllocatedArea {
    /// Virtual address of the page area.
    vma: usize,
    /// Number of pages allocated
    pfn_count: usize,
    // Link to a reverse mapping tree
    link: RbLinks,
}

impl_rblink!(VmaAllocatedArea, let vma: usize = { 0 => link });

pub struct AnonPageMeta {
    parent: Option<Arc<AnonPageMeta>>,
    vma_tree: SpinLock<RbTree<VmaAllocatedArea>>,
}

#[repr(C)]
union InternalPageMetaType {
    anon: ManuallyDrop<Arc<AnonPageMeta>>,
}

pub enum PageMetaVariant<'a> {
    Unused,
    Kernel,
    Anon(&'a mut AnonPageMeta),
}

pub enum PageMetaVariantMut<'a> {
    Unused,
    Kernel,
    Anon(&'a AnonPageMeta),
}

pub struct PageMetaHandle<'a> {
    variant: &'a mut MaybeUninit<InternalPageMetaType>,
    flags: &'a PageFlags,
    // bits [9:6] for daif
    misc: u32,
}

impl<'a> PageMetaHandle<'a> {
    pub fn set_kernel(&mut self) {
        self.flags.set_page_type(1);
    }

    pub fn set_anon(&mut self, anon: Arc<AnonPageMeta>) {
        self.flags.set_page_type(2);

        self.variant.write(InternalPageMetaType {
            anon: ManuallyDrop::new(anon),
        });
    }

    pub fn get(&'a self) -> PageMetaVariantMut<'a> {
        let page_type = self.flags.page_type();

        match page_type {
            0 => PageMetaVariantMut::Unused,
            1 => PageMetaVariantMut::Kernel,
            2 => PageMetaVariantMut::Anon(unsafe { &self.variant.assume_init_ref().anon }),
            _ => panic!("Unsupported page type"),
        }
    }

    pub fn get_mut(&'a mut self) -> PageMetaVariantMut<'a> {
        let page_type = self.flags.page_type();

        match page_type {
            0 => PageMetaVariantMut::Unused,
            1 => PageMetaVariantMut::Kernel,
            2 => PageMetaVariantMut::Anon(unsafe { &mut self.variant.assume_init_mut().anon }),
            _ => panic!("Unsupported page type"),
        }
    }
}

impl Drop for PageMetaHandle<'_> {
    fn drop(&mut self) {
        self.flags.unlock();

        unsafe {
            asm!("msr daif, {x}", x = in(reg) ((self.misc as usize) & 0x3C0));
        };
    }
}

#[repr(C)]
pub struct PageMeta {
    // Counts the number of users of this page.
    refcount: AtomicU32,
    // Various flags, including page locking.
    flags: PageFlags,
    meta: UnsafeCell<MaybeUninit<InternalPageMetaType>>,
}

unsafe impl Send for PageMeta {}
unsafe impl Sync for PageMeta {}

impl PageMeta {
    pub const fn new_unused() -> PageMeta {
        Self {
            refcount: AtomicU32::new(0),
            flags: PageFlags(AtomicU32::new(0)),
            meta: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    pub const fn new_kernel() -> PageMeta {
        Self {
            refcount: AtomicU32::new(0),
            flags: PageFlags(AtomicU32::new(1 << PageFlags::PAGE_TYPE_SHIFT)),
            meta: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    pub fn spin_lock(&self) -> PageMetaHandle<'_> {
        let daif = self.flags.spin_lock() as u32;

        let page_type = self.flags.page_type();
        let variant = unsafe { self.meta.get().as_mut().unwrap() };

        match page_type {
            0 => PageMetaHandle {
                variant,
                flags: &self.flags,
                misc: daif | page_type,
            },
            1 => PageMetaHandle {
                variant,
                flags: &self.flags,
                misc: daif | page_type,
            },
            2 => PageMetaHandle {
                variant,
                flags: &self.flags,
                misc: daif | page_type,
            },
            _ => panic!("Invalid page"),
        }
    }

    pub fn inc_refcount(&self) {
        self.refcount.fetch_add(1, SeqCst);
    }

    pub fn dec_refcount(&self) {
        if self.refcount.fetch_sub(1, SeqCst) == 1 {
            // Drop page information if unused.
            let handle = self.spin_lock();
            let page_type = self.flags.page_type();
            if page_type == 2 {
                unsafe {
                    ManuallyDrop::drop(
                        &mut self.meta.get().as_mut().unwrap().assume_init_mut().anon,
                    );
                }
            }
            // Mark page as unused.
            handle.flags.set_page_type(0);
        }
    }
}
