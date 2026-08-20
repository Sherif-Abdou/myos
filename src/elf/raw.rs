#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ElfHeader {
    pub(crate) ident: ElfHeaderIdent,
    pub(crate) obj_type: u16,
    pub(crate) machine: u16,
    pub(crate) version: u32,
    pub(crate) entry: usize,
    pub(crate) phoff: usize,
    pub(crate) shoff: usize,
    pub(crate) flags: u32,
    pub(crate) ehsize: u16,
    pub(crate) phentsize: u16,
    pub(crate) phnum: u16,
    pub(crate) shentsize: u16,
    pub(crate) shnum: u16,
    pub(crate) shstrndx: u16,
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProgramHeader {
    pub(crate) segment_type: u32,
    pub(crate) flags: u32,
    pub(crate) offset: usize,
    pub(crate) vaddr: usize,
    pub(crate) paddr: usize,
    pub(crate) filesz: usize,
    pub(crate) memsz: usize,
    pub(crate) align: usize,
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SectionHeader {
    pub(crate) name: u32,
    pub(crate) header_type: u32,
    pub(crate) flags: usize,
    pub(crate) addr: usize,
    pub(crate) offset: usize,
    pub(crate) size: usize,
    pub(crate) link: u32,
    pub(crate) info: u32,
    pub(crate) addralign: usize,
    pub(crate) entsize: usize,
}
