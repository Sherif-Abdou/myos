use core::{alloc::Layout, ptr::NonNull};

use alloc::alloc::Allocator;

use crate::{
    allocators::{KBox, KERNEL_ALLOCATOR, align_up},
    elf::raw::{ElfHeader, ProgramHeader, SectionHeader},
    utils::PhysAddr,
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
    Zeroed(NonNull<[u8]>),
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
        let front_padding = header.vaddr % 4096;

        let buffer_len = align_up(front_padding + header.memsz, 4096);
        let mut allocated: NonNull<[u8]> = KERNEL_ALLOCATOR
            .allocate_zeroed(Self::layout(buffer_len))
            .unwrap();
        let buf = unsafe { allocated.as_mut() };

        reader.read(
            header.offset,
            &mut buf[front_padding..(front_padding + header.filesz)],
        );

        Self::Loaded(allocated)
    }

    pub fn load_zeroed(header: &ProgramHeader) -> Self {
        let front_padding = header.vaddr % 4096;

        let buffer_len = align_up(front_padding + header.filesz, 4096);
        let allocated: NonNull<[u8]> = KERNEL_ALLOCATOR
            .allocate_zeroed(Self::layout(buffer_len))
            .unwrap();

        Self::Loaded(allocated)
    }

    pub fn load_from_zeroed(header: &ProgramHeader, reader: impl ElfSource) -> Self {
        let front_padding = header.vaddr % 4096;

        let buffer_len = align_up(front_padding + header.filesz, 4096);
        let mut allocated: NonNull<[u8]> = KERNEL_ALLOCATOR
            .allocate_zeroed(Self::layout(buffer_len))
            .unwrap();
        let buf = unsafe { allocated.as_mut() };

        reader.read(
            header.offset,
            &mut buf[front_padding..(front_padding + header.filesz)],
        );

        Self::Loaded(allocated)
    }
}

impl Drop for SegmentType {
    fn drop(&mut self) {
        if let Self::Loaded(segment) = self {
            unsafe {
                KERNEL_ALLOCATOR.deallocate(segment.cast(), Self::layout(segment.len()));
            }
        } else if let Self::Zeroed(segment) = self {
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
    vaddr: usize,
    memsz: usize,
    filesz: usize,
    permissions: SegmentPermissions,
}

impl Segment {
    pub fn is_null(&self) -> bool {
        matches!(self.blob, SegmentType::Null)
    }

    pub fn virt_addr(&self) -> usize {
        self.vaddr
    }

    pub fn mem_size(&self) -> usize {
        self.memsz
    }

    pub fn mem_page_count(&self) -> usize {
        self.mem_size().div_ceil(4096)
    }

    pub fn loaded_phys_addr(&self) -> PhysAddr {
        let SegmentType::Loaded(ptr) = self.blob else {
            todo!("Only support phys addr of loaded segment.")
        };

        PhysAddr::from(ptr)
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
                blob = SegmentType::load_zeroed(&header);
            } else if disk_len <= mem_len {
                blob = SegmentType::load_from_source(&header, &self.source);
            } else {
                panic!("Cannot handle segment #{}", index);
            }
        } else {
            blob = SegmentType::Null;
        }

        Segment {
            blob,
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
