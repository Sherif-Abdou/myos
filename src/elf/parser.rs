use core::{alloc::Layout, ptr::NonNull};

use alloc::alloc::Allocator;

use crate::{
    allocators::{KBox, KERNEL_ALLOCATOR},
    elf::raw::{ElfHeader, ProgramHeader, SectionHeader},
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

pub struct ElfParser<S: ElfSource> {
    header: ElfHeader,
    source: S,
}

pub enum SegmentType {
    Loaded(NonNull<[u8]>),
    Zeroed(usize),
    Null,
}

unsafe impl Sync for SegmentType {}
unsafe impl Send for SegmentType {}

impl SegmentType {
    const fn layout(len: usize) -> Layout {
        unsafe { Layout::from_size_align_unchecked(len, 4096) }
    }

    pub fn new_loaded(len: usize) -> Self {
        let allocated: NonNull<[u8]> = KERNEL_ALLOCATOR.allocate_zeroed(Self::layout(len)).unwrap();

        Self::Loaded(allocated)
    }

    pub fn load_from_source(header: &ProgramHeader, reader: impl ElfSource) -> Self {
        let mut allocated: NonNull<[u8]> = KERNEL_ALLOCATOR
            .allocate_zeroed(Self::layout(header.filesz))
            .unwrap();
        let buf = unsafe { allocated.as_mut() };

        reader.read(header.offset, buf);

        Self::Loaded(allocated)
    }
}

impl Drop for SegmentType {
    fn drop(&mut self) {
        if let Self::Loaded(segment) = self {
            unsafe {
                KERNEL_ALLOCATOR.deallocate(segment.cast(), Self::layout(segment.len()));
            }
        }
    }
}

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

pub struct Segment {
    blob: SegmentType,
    memsz: usize,
    filesz: usize,
    permissions: SegmentPermissions,
}

impl Segment {
    pub fn is_null(&self) -> bool {
        matches!(self.blob, SegmentType::Null)
    }

    pub fn phys_addr(&self) -> usize {
        let SegmentType::Loaded(ref ptr) = self.blob else {
            todo!("Only support phys addr of loaded segment.")
        };

        ptr.addr().get() & 0x7fffffffff
    }
}

impl<S: ElfSource> ElfParser<S> {
    pub fn new(source: S) -> Self {
        let mut buf = [0u8; core::mem::size_of::<ElfHeader>()];
        source.read(0, &mut buf);
        let ptr = buf.as_ptr();

        let header = unsafe { ptr.cast::<ElfHeader>().read() };
        Self { header, source }
    }

    fn elf_header(&self) -> &ElfHeader {
        &self.header
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
        if header.segment_type == 1 {
            let mem_len = header.memsz;
            let disk_len = header.filesz;

            if mem_len == 0 {
                Segment {
                    blob: SegmentType::Zeroed(mem_len),
                    memsz: header.memsz,
                    filesz: header.filesz,
                    permissions: SegmentPermissions::from_header(&header),
                }
            } else if disk_len == mem_len {
                let segment_buffer = SegmentType::load_from_source(&header, &self.source);

                Segment {
                    blob: segment_buffer,
                    memsz: header.memsz,
                    filesz: header.filesz,
                    permissions: SegmentPermissions::from_header(&header),
                }
            } else {
                panic!("Cannot handle segment #{}", index);
            }
        } else {
            Segment {
                blob: SegmentType::Null,
                memsz: header.memsz,
                filesz: header.filesz,
                permissions: SegmentPermissions::from_header(&header),
            }
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
