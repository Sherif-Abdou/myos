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

const ADDR_MASK: u64 = 0x0000ffffffff0000;

#[repr(transparent)]
struct ArmPageDescriptor(u64);

impl ArmPageDescriptor {
    pub const fn new() -> Self {
        Self(0x1)
    }

    pub const fn is_page_descriptor(&self) -> bool {
        (self.0 & 0b10) != 0
    }

    pub const fn set_page_descriptor(&mut self, descriptor_type: bool) {
        self.0 &= !((descriptor_type as u64) << 1);
        self.0 |= (descriptor_type as u64) << 1;
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
}

#[repr(align(4096))]
pub struct ArmDescriptorGroup([ArmPageDescriptor; 512]);

impl ArmDescriptorGroup {
    pub const fn new() -> Self {
        Self([const { ArmPageDescriptor::new() }; 512])
    }

    pub fn phys_addr(&self) -> u64 {
        (self.0.as_ptr().addr() as u64) & 0x7fffffffff
    }
}
