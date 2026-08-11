use core::{
    alloc::Layout,
    arch::asm,
    ops::{Index, IndexMut},
    ptr::NonNull,
};

use alloc::alloc::Allocator;

use crate::{
    allocators::KERNEL_ALLOCATOR,
    impl_link,
    sched::Mutex,
    utils::{Arc, List, ListLinks, SpinLock, UniqueArc, with_core_critical_section},
};

/// Bit 63 — AMEC (encrypted-memory context select, FEAT_MEC).
pub const AMEC_OFFSET: u64 = 63;
pub const AMEC_MASK: u64 = 0x1 << AMEC_OFFSET;

/// Bits 62:60 — PBHA[3:1], or POIndex[2:0] (FEAT_S1POE).
pub const PBHA_3_1_OFFSET: u64 = 60;
pub const PBHA_3_1_MASK: u64 = 0x7 << PBHA_3_1_OFFSET;
pub const PO_INDEX_OFFSET: u64 = PBHA_3_1_OFFSET;
pub const PO_INDEX_MASK: u64 = PBHA_3_1_MASK;

/// Bit 59 — PBHA[0], or AttrIndx[3] (FEAT_AIE).
pub const PBHA_0_OFFSET: u64 = 59;
pub const PBHA_0_MASK: u64 = 0x1 << PBHA_0_OFFSET;
pub const ATTR_INDX_3_OFFSET: u64 = PBHA_0_OFFSET;
pub const ATTR_INDX_3_MASK: u64 = PBHA_0_MASK;

/// Bits 58:55 — IGNORED, available for software use.
pub const IGNORED_58_55_OFFSET: u64 = 55;
pub const IGNORED_58_55_MASK: u64 = 0xF << IGNORED_58_55_OFFSET;

/// Bit 55 — reserved for software use (within the ignored range above).
pub const SW_USE_OFFSET: u64 = 55;
pub const SW_USE_MASK: u64 = 0x1 << SW_USE_OFFSET;

/// Bit 54 — UXN, or XN, or PXN (alt layout), or PIIndex[3] (FEAT_S1PIE).
pub const UXN_OFFSET: u64 = 54;
pub const UXN_MASK: u64 = 0x1 << UXN_OFFSET;
pub const XN_OFFSET: u64 = UXN_OFFSET;
pub const XN_MASK: u64 = UXN_MASK;
pub const PXN_ALT_OFFSET: u64 = UXN_OFFSET;
pub const PXN_ALT_MASK: u64 = UXN_MASK;
pub const PI_INDEX_3_OFFSET: u64 = UXN_OFFSET;
pub const PI_INDEX_3_MASK: u64 = UXN_MASK;

/// Bit 53 — PXN, or PIIndex[2].
pub const PXN_OFFSET: u64 = 53;
pub const PXN_MASK: u64 = 0x1 << PXN_OFFSET;
pub const PI_INDEX_2_OFFSET: u64 = PXN_OFFSET;
pub const PI_INDEX_2_MASK: u64 = PXN_MASK;

/// Bit 52 — Contiguous hint, or Protected (alt layout).
pub const CONTIGUOUS_OFFSET: u64 = 52;
pub const CONTIGUOUS_MASK: u64 = 0x1 << CONTIGUOUS_OFFSET;
pub const PROTECTED_OFFSET: u64 = CONTIGUOUS_OFFSET;
pub const PROTECTED_MASK: u64 = CONTIGUOUS_MASK;

/// Bit 51 — DBM (Dirty Bit Modifier), or PIIndex[1].
pub const DBM_OFFSET: u64 = 51;
pub const DBM_MASK: u64 = 0x1 << DBM_OFFSET;
pub const PI_INDEX_1_OFFSET: u64 = DBM_OFFSET;
pub const PI_INDEX_1_MASK: u64 = DBM_MASK;

/// Bit 50 — GP (Guarded Page, BTI).
pub const GP_OFFSET: u64 = 50;
pub const GP_MASK: u64 = 0x1 << GP_OFFSET;

/// Bit 16 — nT (block-entry "no translation" hint).
pub const N_T_OFFSET: u64 = 16;
pub const N_T_MASK: u64 = 0x1 << N_T_OFFSET;

/// Bits 15:12 — OA[51:48] (FEAT_LPA2).
pub const OA_51_48_OFFSET: u64 = 12;
pub const OA_51_48_MASK: u64 = 0xF << OA_51_48_OFFSET;

/// Bit 11 — nG (not-Global), or NSE (FEAT_RME, alt layout).
pub const NG_OFFSET: u64 = 11;
pub const NG_MASK: u64 = 0x1 << NG_OFFSET;
pub const NSE_OFFSET: u64 = NG_OFFSET;
pub const NSE_MASK: u64 = NG_MASK;

/// Bit 10 — AF (Access Flag).
pub const AF_OFFSET: u64 = 10;
pub const AF_MASK: u64 = 0x1 << AF_OFFSET;

/// Bits 9:8 — SH[1:0] (Shareability), or OA[51:50] (alt layout).
pub const SH_OFFSET: u64 = 8;
pub const SH_MASK: u64 = 0x3 << SH_OFFSET;
pub const OA_51_50_OFFSET: u64 = SH_OFFSET;
pub const OA_51_50_MASK: u64 = SH_MASK;

/// Bit 7 — AP[2] (Access Permission bit 2), or nDirty (FEAT_S1PIE, alt layout).
pub const AP2_OFFSET: u64 = 7;
pub const AP2_MASK: u64 = 0x1 << AP2_OFFSET;
pub const N_DIRTY_OFFSET: u64 = AP2_OFFSET;
pub const N_DIRTY_MASK: u64 = AP2_MASK;

/// Bit 6 — AP[1] (Access Permission bit 1), or PIIndex[0] (alt layout).
pub const AP1_OFFSET: u64 = 6;
pub const AP1_MASK: u64 = 0x1 << AP1_OFFSET;
pub const PI_INDEX_0_OFFSET: u64 = AP1_OFFSET;
pub const PI_INDEX_0_MASK: u64 = AP1_MASK;

/// Bit 5 — NS (Non-Secure).
pub const NS_OFFSET: u64 = 5;
pub const NS_MASK: u64 = 0x1 << NS_OFFSET;

/// Bits 4:2 — AttrIndx[2:0] (index into MAIR_ELx).
pub const ATTR_INDX_OFFSET: u64 = 2;
pub const ATTR_INDX_MASK: u64 = 0x7 << ATTR_INDX_OFFSET;

const ADDR_MASK: u64 = 0x0000fffffffff000;

const NUM_DESCRIPTORS_IN_PAGE: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLayer {
    First,
    Second,
    Third,
}

impl PageLayer {
    /// Get the next smallest descriptor size after this.
    fn next_layer(&self) -> Self {
        match self {
            PageLayer::First => PageLayer::Second,
            PageLayer::Second => PageLayer::Third,
            PageLayer::Third => PageLayer::Third,
        }
    }

    fn is_lowest_layer(&self) -> bool {
        matches!(self, PageLayer::Third)
    }

    /// Get the size of the region covered by this layer of descriptor.
    fn get_region_size(&self) -> usize {
        match self {
            PageLayer::First => 1024 * 1024 * 1024,
            PageLayer::Second => 2 * 1024 * 1024,
            PageLayer::Third => 4096,
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArmPageDescriptor(u64);

impl ArmPageDescriptor {
    pub const fn new() -> Self {
        Self(0x1)
    }

    pub const fn is_page_descriptor(&self) -> bool {
        (self.0 & 0b10) != 0
    }

    pub const fn set_page_descriptor(&mut self, descriptor_type: bool) {
        self.0 &= !(0b10);
        self.0 |= (descriptor_type as u64) << 1;
    }

    pub const fn valid(&self) -> bool {
        (self.0 & 0b1) != 0
    }

    pub const fn set_valid(&mut self, valid: bool) {
        self.0 &= !0b1;
        self.0 |= valid as u64;
    }

    pub const fn af(&self) -> bool {
        (self.0 & AF_MASK) != 0
    }

    pub const fn set_af(&mut self, af: bool) {
        self.0 &= !AF_MASK;
        self.0 |= (af as u64) << AF_OFFSET;
    }

    pub const fn set_phys_address(&mut self, addr: usize) {
        self.0 &= !ADDR_MASK;
        self.0 |= ADDR_MASK & (addr as u64);
    }

    pub const fn get_phys_address(&self) -> usize {
        (self.0 & ADDR_MASK) as usize
    }

    pub const fn get_attr_group(&self) -> u64 {
        (self.0 & ATTR_INDX_MASK) >> ATTR_INDX_OFFSET
    }

    pub const fn set_attr_group(&mut self, group: u64) {
        self.0 &= !ATTR_INDX_MASK;
        self.0 |= (group << ATTR_INDX_OFFSET) & ATTR_INDX_OFFSET;
    }

    pub fn break_before_make(&mut self, func: impl FnOnce(ArmPageDescriptor) -> ArmPageDescriptor) {
        with_core_critical_section(|| {
            let new_descriptor = func(*self);

            unsafe {
                asm!(r#"
                str xzr, [{0}]
                dsb ishst
                tlbi vmalle1is
                dsb ish
                str {1}, [{0}]
                dsb ishst
                isb
                "#, in(reg) ((&raw mut self.0).addr()), in(reg) (new_descriptor.0));
            }
        });
    }
}

#[repr(align(4096))]
struct ArmDescriptorGroup([ArmPageDescriptor; NUM_DESCRIPTORS_IN_PAGE]);

impl IndexMut<usize> for ArmDescriptorGroup {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl Index<usize> for ArmDescriptorGroup {
    type Output = ArmPageDescriptor;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl ArmDescriptorGroup {
    pub const fn new() -> Self {
        Self([const { ArmPageDescriptor::new() }; NUM_DESCRIPTORS_IN_PAGE])
    }

    /// Returns the physical address of the descriptor group.
    pub fn phys_addr(&self) -> u64 {
        (self.0.as_ptr().addr() as u64) & 0x7fffffffff
    }
}

const KERNEL_START_VIRT_ADDR: usize = !0x7fffffffff;

pub struct ArmPageTableRoot {
    root_group: ArmDescriptorGroupManager,
}

impl ArmPageTableRoot {
    pub fn create_kernel_root() -> Self {
        let mut group = ArmDescriptorGroupManager::new(0, PageLayer::First);
        let region_size = PageLayer::First.get_region_size();

        for (i, descriptor) in unsafe { group.descriptors.as_mut().0.iter_mut().enumerate() } {
            descriptor.set_attr_group(0);
            descriptor.set_phys_address(i * region_size);
            descriptor.set_af(true);
            descriptor.set_page_descriptor(false);
            descriptor.set_valid(true);
        }

        Self { root_group: group }
    }

    /// Gets the descriptor for a particular vma, at the Layer 3 portion of the page.
    ///
    /// Makes the layers if needed.
    pub fn descriptor_for_vma(&self, addr: usize) -> (Arc<ArmDescriptorGroupManager>, usize) {
        assert!(addr >= KERNEL_START_VIRT_ADDR);
        let relative_addr = addr - KERNEL_START_VIRT_ADDR;

        let region_size = self.root_group.layer.lock().get_region_size();
        let index = relative_addr / region_size;

        let sub_group = self.root_group.subgroup(index);

        if let Some(sub_group) = sub_group {
            let region_size = sub_group.layer.lock().get_region_size();
            let index = (relative_addr / region_size) % NUM_DESCRIPTORS_IN_PAGE;
            let page_group = sub_group.subgroup(index);
            if let Some(page_group) = page_group {
                let index = (relative_addr / 4096) % NUM_DESCRIPTORS_IN_PAGE;
                (page_group, index)
            } else {
                sub_group.split_block_descriptor(index);
                self.descriptor_for_vma(addr)
            }
        } else {
            self.root_group.split_block_descriptor(index);
            self.descriptor_for_vma(addr)
        }
    }

    /// Binds this page table as the kernel page table. This should really only be called once.
    pub fn bind_kernel(&self) {
        with_core_critical_section(|| {
            let phys_addr = self.root_group.descriptors.addr().get() & 0x7fffffffff;

            unsafe {
                asm!(r#"
            isb
            tlbi vmalle1is
            dsb ish
            isb
            msr ttbr1_el1, {0}
            isb
            tlbi vmalle1is
            dsb ish
            isb
            "#, in(reg) (phys_addr));
            }
        });
    }
}

pub struct ArmDescriptorGroupManager {
    index: usize,
    descriptors: NonNull<ArmDescriptorGroup>,
    children: Mutex<List<ArmDescriptorGroupManager>>,
    layer: SpinLock<PageLayer>,
    links: ListLinks,
}

impl ArmDescriptorGroupManager {
    pub fn map_page(&self, index: usize, phys_addr: usize) {
        assert!(phys_addr.is_multiple_of(4096));

        unsafe {
            // TODO: Is this sketch?
            let group = self.descriptors.as_ptr().as_mut().unwrap();
            group[index].break_before_make(|mut before| {
                before.set_af(true);
                before.set_page_descriptor(true);
                before.set_valid(true);
                before.set_phys_address(phys_addr);
                before
            });
        }
    }

    pub fn is_lowest_layer(&self) -> bool {
        self.layer.lock().is_lowest_layer()
    }

    pub fn split_block_descriptor(&self, index: usize) {
        if self.layer.lock().is_lowest_layer() {
            return;
        }

        let descriptor = unsafe { self.descriptors.as_ref()[index] };
        let phys_addr = descriptor.get_phys_address();

        let current_layer = *self.layer.lock();
        let next_layer = current_layer.next_layer();
        assert_ne!(current_layer, next_layer);

        let mut new_group = Self::new(index, next_layer);

        let new_group_phys_addr = new_group.descriptors.addr().get();

        let region_size = next_layer.get_region_size();
        for (i, new_descriptor) in
            unsafe { new_group.descriptors.as_mut().0.iter_mut().enumerate() }
        {
            new_descriptor.set_phys_address(phys_addr + i * region_size);
            new_descriptor.set_af(true);
            new_descriptor.set_page_descriptor(next_layer.is_lowest_layer());
            new_descriptor.set_valid(true);
        }

        let mut children = self.children.lock();
        let mut cursor = children.cursor_mut();

        while let Some(group) = cursor.get()
            && group.index < index
        {
            let _ = cursor.next();
        }

        cursor.insert_before(UniqueArc::new(new_group).into());

        unsafe {
            // TODO: Is this sketch?
            let group = self.descriptors.as_ptr().as_mut().unwrap();
            group[index].break_before_make(|mut before| {
                before.set_phys_address(new_group_phys_addr & 0x7fffffffff);
                before.set_page_descriptor(true);
                before.set_valid(true);
                before.set_af(true);
                before
            });
        }
    }

    pub fn subgroup(&self, index: usize) -> Option<Arc<ArmDescriptorGroupManager>> {
        let mut children = self.children.lock();
        let mut cursor = children.cursor_mut();

        while let Some(group) = cursor.get()
            && group.index != index
        {
            let _ = cursor.next();
        }

        cursor.get_arc()
    }

    fn new(index: usize, layer: PageLayer) -> Self {
        let allocation = KERNEL_ALLOCATOR
            .allocate_zeroed(Layout::new::<ArmDescriptorGroup>())
            .unwrap();
        let descriptors: NonNull<ArmDescriptorGroup> = allocation.cast();

        Self {
            index,
            descriptors,
            children: Mutex::new(List::new()),
            layer: SpinLock::new(layer),
            links: ListLinks::new(),
        }
    }
}

impl Drop for ArmDescriptorGroupManager {
    fn drop(&mut self) {
        unsafe {
            KERNEL_ALLOCATOR
                .deallocate(self.descriptors.cast(), Layout::new::<ArmDescriptorGroup>());
        }
    }
}

impl_link!(ArmDescriptorGroupManager, 0 => links);
