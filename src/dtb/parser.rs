use core::{ffi::CStr, ptr::NonNull};

use alloc::collections::LinkedList;

use crate::{
    allocators::{BUMP_ALLOCATOR, BumpAllocator},
    utils::CoreLock,
};

#[derive(Debug)]
struct FtdHeader {
    magic: u32,
    total_size: u32,
    off_dt_struct: u32,
    off_dt_string: u32,
    off_mem_rsvmap: u32,
    version: u32,
    last_comp_version: u32,
    boot_cpuid_phys: u32,
    size_dt_strings: u32,
    size_dt_structs: u32,
}

impl FtdHeader {
    pub fn parse(start: NonNull<u8>) -> Self {
        let start = start.cast::<u32>();
        unsafe {
            let magic = start.offset(0).read().swap_bytes();
            let total_size = start.offset(1).read().swap_bytes();
            let off_dt_struct = start.offset(2).read().swap_bytes();
            let off_dt_string = start.offset(3).read().swap_bytes();
            let off_mem_rsvmap = start.offset(4).read().swap_bytes();
            let version = start.offset(5).read().swap_bytes();
            let last_comp_version = start.offset(6).read().swap_bytes();
            let boot_cpuid_phys = start.offset(7).read().swap_bytes();
            let size_dt_strings = start.offset(8).read().swap_bytes();
            let size_dt_structs = start.offset(9).read().swap_bytes();

            Self {
                magic,
                total_size,
                off_dt_struct,
                off_dt_string,
                off_mem_rsvmap,
                version,
                last_comp_version,
                boot_cpuid_phys,
                size_dt_strings,
                size_dt_structs,
            }
        }
    }
}

#[derive(Debug)]
pub struct FdtProp {
    name: &'static str,
    value: Option<NonNull<[u8]>>,
}

impl FdtProp {
    pub fn parse(cursor: &mut NonNull<u32>, strings: NonNull<u8>) -> Self {
        assert_eq!(unsafe { cursor.read().swap_bytes() }, 0x3);
        *cursor = unsafe { cursor.offset(1) };

        let len = unsafe { cursor.read().swap_bytes() };
        *cursor = unsafe { cursor.offset(1) };

        let name_offset = unsafe { cursor.read().swap_bytes() };
        *cursor = unsafe { cursor.offset(1) };

        let name: &'static str = unsafe {
            CStr::from_ptr(strings.offset(name_offset as isize).as_ptr())
                .to_str()
                .unwrap()
        };

        if len > 0 {
            let words = len.div_ceil(4);
            let value = Some(NonNull::slice_from_raw_parts(cursor.cast(), len as usize));

            *cursor = unsafe { cursor.offset(words as isize) };

            Self { value, name }
        } else {
            Self { value: None, name }
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn read_prop_string(&self) -> Option<&'static str> {
        self.value.map(|ptr| {
            unsafe { CStr::from_ptr(ptr.as_ptr().cast()) }
                .to_str()
                .unwrap()
        })
    }

    pub fn read_u32(&self, index: usize) -> Option<u32> {
        self.value.map(|ptr| unsafe {
            ptr.as_ptr()
                .cast::<u32>()
                .offset(index as isize)
                .read()
                .swap_bytes()
        })
    }

    pub fn read_u64(&self, index: usize) -> Option<u64> {
        let base_offset = 2 * index as isize;
        self.value.map(|ptr| {
            let upper = unsafe {
                ptr.as_ptr()
                    .cast::<u32>()
                    .offset(base_offset)
                    .read()
                    .swap_bytes()
            } as u64;
            let lower = unsafe {
                ptr.as_ptr()
                    .cast::<u32>()
                    .offset(base_offset + 1)
                    .read()
                    .swap_bytes()
            } as u64;

            (upper << 32) | lower
        })
    }
}

#[derive(Debug)]
pub struct FdtNodeMeta {
    // # of u32 cells used for the size of registers
    size_cells: u32,
    // # of u32 cells used for the size of addresses
    address_cells: u32,
    /// Phandle of the interrupt-parent
    interrupt_parent: Option<u32>,
}

impl Default for FdtNodeMeta {
    fn default() -> Self {
        Self {
            size_cells: 1,
            address_cells: 2,
            interrupt_parent: None,
        }
    }
}

pub struct FdtNode {
    name: Option<&'static str>,
    properties: LinkedList<FdtProp, &'static CoreLock<BumpAllocator>>,
    children: LinkedList<FdtNode, &'static CoreLock<BumpAllocator>>,
}

impl FdtNode {
    pub fn name(&self) -> &'static str {
        self.name.unwrap_or("")
    }

    pub fn parse(cursor: &mut NonNull<u32>, strings: NonNull<u8>) -> Self {
        assert_eq!(unsafe { cursor.read().swap_bytes() }, 0x1);
        *cursor = unsafe { cursor.offset(1) };
        let mut name = None;
        if unsafe { cursor.read().swap_bytes() } != 0x0 {
            let cstr = unsafe { CStr::from_ptr(cursor.cast().as_ptr()) };
            let len = cstr.count_bytes() + 1;
            let jump = len.div_ceil(4);
            name = Some(cstr.to_str().unwrap());
            *cursor = unsafe { cursor.offset(jump as isize) };
        } else {
            *cursor = unsafe { cursor.offset(1) };
        }

        let mut properties = LinkedList::new_in(BUMP_ALLOCATOR.get());
        let mut children = LinkedList::new_in(BUMP_ALLOCATOR.get());

        while unsafe { cursor.read().swap_bytes() } != 0x2 {
            match unsafe { cursor.read().swap_bytes() } {
                0x1 => {
                    children.push_back(FdtNode::parse(cursor, strings));
                }
                0x3 => {
                    properties.push_back(FdtProp::parse(cursor, strings));
                }
                0x4 => {
                    *cursor = unsafe { cursor.offset(1) };
                }
                _ => panic!("Unknown pattern."),
            }
        }
        *cursor = unsafe { cursor.offset(1) };

        Self {
            name,
            properties,
            children,
        }
    }

    pub fn properties(&self) -> impl Iterator<Item = &FdtProp> {
        self.properties.iter()
    }

    pub fn children(&self) -> impl Iterator<Item = &FdtNode> {
        self.children.iter()
    }

    pub fn find_child_by_name(&self, name: &str) -> Option<&FdtNode> {
        self.children().find(|node| node.name() == name)
    }

    fn find_prop_by_name(&self, name: &str) -> Option<&FdtProp> {
        self.properties().find(|prop| prop.name() == name)
    }

    pub fn read_prop_string(&self, name: &str) -> Option<&'static str> {
        self.find_prop_by_name(name)
            .and_then(|prop| prop.read_prop_string())
    }

    pub fn read_u32(&self, name: &str, index: usize) -> Option<u32> {
        self.find_prop_by_name(name)
            .and_then(|prop| prop.read_u32(index))
    }

    pub fn read_u64(&self, name: &str, index: usize) -> Option<u64> {
        self.find_prop_by_name(name)
            .and_then(|prop| prop.read_u64(index))
    }
}

pub struct Fdt {
    header: FtdHeader,
    nodes: LinkedList<FdtNode, &'static CoreLock<BumpAllocator>>,
}

unsafe extern "C" {
    static __dtb_addr: usize;
}

impl Fdt {
    fn parse(start: NonNull<u8>) -> Self {
        let header = FtdHeader::parse(start);

        let strings = unsafe { start.byte_offset(header.off_dt_string as isize) };
        let structs = unsafe { start.byte_offset(header.off_dt_struct as isize) };
        let mut cursor: NonNull<u32> = structs.cast();

        let mut nodes = LinkedList::new_in(BUMP_ALLOCATOR.get());
        while unsafe { cursor.read().swap_bytes() != 0x09 } {
            nodes.push_back(FdtNode::parse(&mut cursor, strings));
        }

        Self { header, nodes }
    }

    pub fn from_boot() -> Self {
        let start = NonNull::new(unsafe { __dtb_addr as _ }).unwrap();

        Self::parse(start)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &FdtNode> {
        self.nodes.iter()
    }

    pub fn root(&self) -> &FdtNode {
        self.nodes().next().expect("Missing FDT root.")
    }
}
