use crate::{
    allocators::KBox,
    elf::raw::{ElfHeader, ProgramHeader, SectionHeader},
    memory::PAGE_SIZE,
    printk,
    sched::LazyPageBufferSource,
    subsystem::Inode,
    utils::Arc,
};

pub trait ElfSource {
    fn read(&self, offset: usize, buf: &mut [u8]);
}

impl ElfSource for &[u8] {
    fn read(&self, offset: usize, buf: &mut [u8]) {
        buf.copy_from_slice(&self[offset..(offset + buf.len())]);
    }
}

impl ElfSource for KBox<[u8]> {
    fn read(&self, offset: usize, buf: &mut [u8]) {
        buf.copy_from_slice(&self[offset..(offset + buf.len())]);
    }
}

impl<T: ElfSource> ElfSource for &T {
    fn read(&self, offset: usize, buf: &mut [u8]) {
        (*self).read(offset, buf)
    }
}

impl<T: ElfSource> ElfSource for Arc<T> {
    fn read(&self, offset: usize, buf: &mut [u8]) {
        T::read(self, offset, buf);
    }
}

impl ElfSource for Inode {
    fn read(&self, offset: usize, buf: &mut [u8]) {
        // TODO: error handle
        let _ = Inode::read(self, offset as u64, buf);
    }
}

type ElfSourceArc = Arc<dyn ElfSource + Send + Sync + 'static>;

pub struct ElfParser {
    header: ElfHeader,
    source: ElfSourceArc,
}

#[derive(Clone)]
pub enum SegmentType {
    Loaded(Arc<dyn ElfSource + 'static>),
    Zeroed,
    Null,
}

unsafe impl Sync for SegmentType {}
unsafe impl Send for SegmentType {}

impl SegmentType {}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SegmentPermissions(u32);

impl SegmentPermissions {
    pub fn new() -> Self {
        Self(0)
    }

    pub const fn read(&self) -> bool {
        (self.0 & 0b100) != 0
    }

    pub const fn write(&self) -> bool {
        (self.0 & 0b10) != 0
    }

    pub const fn execute(&self) -> bool {
        (self.0 & 0b1) != 0
    }

    pub fn from_header(header: &ProgramHeader) -> Self {
        Self(header.flags)
    }
}

#[derive(Clone)]
pub struct Segment {
    source: SegmentType,
    offset: usize,
    vaddr: usize,
    memsz: usize,
    filesz: usize,
    permissions: SegmentPermissions,
}

impl Segment {
    pub fn is_null(&self) -> bool {
        matches!(self.source, SegmentType::Null)
    }

    pub fn virt_addr(&self) -> usize {
        self.vaddr
    }

    pub fn mem_size(&self) -> usize {
        self.memsz
    }

    pub fn offset(&self) -> usize {
        self.vaddr % PAGE_SIZE
    }

    pub fn mem_page_count(&self) -> usize {
        (self.offset() + self.mem_size()).div_ceil(4096)
    }
}

impl LazyPageBufferSource for Segment {
    fn read_page(&self, offset: usize, buf: &mut [u8]) {
        match self.source {
            SegmentType::Loaded(ref elf_source) => {
                let front_padding = self.vaddr % PAGE_SIZE;

                let end = self.filesz.saturating_sub(offset).min(PAGE_SIZE);
                if offset == 0 {
                    elf_source.read(self.offset + offset, &mut buf[front_padding..(front_padding + end)]);
                    buf[(front_padding + end)..].fill(0);
                } else {
                    elf_source.read(
                        self.offset + offset.saturating_sub(front_padding),
                        &mut buf[..end],
                    );
                    buf[end..].fill(0);
                }
            }
            SegmentType::Zeroed => {
                buf.fill(0);
            }
            SegmentType::Null => {}
        }
    }
}

impl ElfParser {
    pub fn new(source: Arc<dyn ElfSource + Send + Sync + 'static>) -> Self {
        let mut buf = [0u8; core::mem::size_of::<ElfHeader>()];
        source.read(0, &mut buf);
        let ptr = buf.as_ptr();

        let header = unsafe { ptr.cast::<ElfHeader>().read() };
        Self { header, source }
    }

    fn elf_header(&self) -> &ElfHeader {
        &self.header
    }

    pub fn entry_vma(&self) -> usize {
        self.elf_header().entry
    }

    fn num_program_headers(&self) -> u16 {
        self.elf_header().phnum
    }

    fn num_section_headers(&self) -> u16 {
        self.elf_header().shnum
    }

    fn program_header(&self, index: usize) -> ProgramHeader {
        let mut buf = [0u8; core::mem::size_of::<ProgramHeader>()];
        let offset = core::mem::size_of::<ProgramHeader>() * index + self.elf_header().phoff;
        self.source.read(offset, &mut buf);
        let ptr = buf.as_ptr();

        unsafe { ptr.cast::<ProgramHeader>().read() }
    }

    pub fn num_segments(&self) -> usize {
        self.num_program_headers() as usize
    }

    pub fn segment(&self, index: usize) -> Segment {
        let header = self.program_header(index);
        let blob;
        if header.segment_type == 1 {
            let mem_len = header.memsz;
            let disk_len = header.filesz;

            if mem_len == 0 {
                blob = SegmentType::Zeroed;
            } else if disk_len <= mem_len {
                blob = SegmentType::Loaded(self.source.clone());
            } else {
                panic!("Cannot handle segment #{}", index);
            }
        } else {
            blob = SegmentType::Null;
        }

        Segment {
            source: blob,
            offset: header.offset,
            vaddr: header.vaddr,
            memsz: header.memsz,
            filesz: header.filesz,
            permissions: SegmentPermissions::from_header(&header),
        }
    }

    fn section_headers(&self, index: usize) -> SectionHeader {
        let mut buf = [0u8; core::mem::size_of::<SectionHeader>()];
        let offset = core::mem::size_of::<SectionHeader>() * index + self.elf_header().phoff;
        self.source.read(offset, &mut buf);
        let ptr = buf.as_ptr();

        unsafe { ptr.cast::<SectionHeader>().read() }
    }
}
