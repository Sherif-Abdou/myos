#[repr(C)]
pub(crate) struct ElfHeaderIdent {
    pub(crate) magic: [u8; 4],
    pub(crate) class: u8,
    pub(crate) data: u8,
    pub(crate) version: u8,
    pub(crate) abi: u8,
    pub(crate) abi_version: u8,
    _pad: [u8; 7],
}

#[repr(C)]
pub(crate) struct ElfHeader {
    pub(crate) ident: ElfHeaderIdent,
    pub(crate) obj_type: u16,
    pub(crate) machine: u16,
    pub(crate) version: u32,
    pub(crate) entry: usize,
    pub(crate) off: usize,
    // TODO: finish
}
