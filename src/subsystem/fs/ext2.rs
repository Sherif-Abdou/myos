//! Revision 1 Ext2 FS support (no features supported).

use core::ffi::CStr;

use crate::{
    impl_link,
    sched::Mutex,
    subsystem::{FsResult, Inode, InodeOperations, block_cache},
    utils::{Arc, List, ListArc, ListLinkWrapper, ListLinks, UniqueArc},
};

use super::FsError;

#[repr(C)]
struct SuperBlock {
    /// Total number of inodes, used + free, in the fs.
    inodes_count: u32,
    /// Total number of blocks, used + free, in the fs.
    blocks_count: u32,
    /// Total number of blocks reserved for super user.
    r_blocks_count: u32,
    /// Total number of free blocks.
    free_blocks_count: u32,
    /// Total number of free inodes.
    free_inodes_count: u32,
    /// ID of the first datablock.
    first_data_block: u32,
    /// log2(max block size) - 10
    log_block_size: u32,
    /// log2(max frag size) - 10
    log_frag_size: u32,
    /// Total number of blocks per group.
    blocks_per_group: u32,
    /// Total number of fragments per group.
    frags_per_group: u32,
    /// Totla number of inodes per group.
    inodes_per_group: u32,
    /// Unix time of last time mounted.
    mtime: u32,
    /// Unix time of last write.
    wtime: u32,
    /// How many times the file system was mounted before last verify.
    mnt_count: u16,
    /// Max times to mount before verification.
    max_mnt_count: u16,
    /// Ext2 identifcation magic number. 0xEF53.
    magic: u16,
    /// Indicates file system state.
    state: u16,
    /// What the driver should do if errors are detected.
    errors: u16,
    /// Minor revision level.
    minor_rev_level: u16,
    /// Unix time of last fs check.
    last_check: u32,
    /// Max Unix time interval between fs checks.
    check_interval: u32,
    /// Identifier of OS that created file system.
    creator_os: u32,
    /// Revision level value.
    rev_level: u32,
    /// Default user id for reserved blocks.
    def_resuid: u16,
    /// Default group id for reserved blocks.
    def_resgid: u16,
    /// First usable Inode index (usually 11).
    first_inode: u32,
    /// Inode size
    inode_size: u16,
    /// Block group number hosting this superblock structure.
    block_group_number: u16,
    /// Compatible features of this filesystem.
    ///
    /// FS can be loaded even if these features aren't supported.
    feature_compatible: u32,
    /// Incompatible features of this filesystem.
    ///
    /// FS can not be loaded if these features aren't supported.
    feature_incompatible: u32,
    /// Read-only compatible features of this filesystem.
    ///
    ///
    /// FS can only be ro mounted if these features aren't supported.
    feature_ro_compatible: u32,
    /// UUID of this FS
    uuid: [u8; 16],
    /// Name of this volume. Should be 0 terminated.
    volume_name: [u8; 16],
    /// Directory where this was last mounted. Should be 0 terminated.
    last_mounted: [u8; 64],
    /// Used to determine compression algorithm used.
    algo_bitmap: u32,
}

impl SuperBlock {
    fn block_size(&self) -> u64 {
        1024 << (self.log_block_size as u64)
    }

    fn group_count(&self) -> u64 {
        (self.blocks_count / self.blocks_per_group) as u64
    }

    fn super_block_offset(&self) -> u64 {
        0
    }

    fn group_descriptor_offset(&self) -> u64 {
        self.super_block_offset() + 1
    }

    fn group_descriptor_blocks(&self) -> u64 {
        (self.group_count() * 32).div_ceil(self.block_size())
    }

    fn block_bitmap_offset(&self) -> u64 {
        self.group_descriptor_offset() + self.group_descriptor_blocks()
    }

    fn block_bitmap_blocks(&self) -> u64 {
        (self.blocks_per_group as u64).div_ceil(self.block_size() * 8)
    }

    fn inode_bitmap_offset(&self) -> u64 {
        self.block_bitmap_offset() + self.block_bitmap_blocks()
    }

    fn inode_bitmap_blocks(&self) -> u64 {
        (self.inodes_per_group as u64).div_ceil(self.block_size() * 8)
    }

    fn inode_table_offset(&self) -> u64 {
        self.inode_bitmap_offset() + self.inode_bitmap_blocks()
    }

    fn inode_table_blocks(&self) -> u64 {
        (self.inodes_per_group as u64 * self.inode_size as u64).div_ceil(self.block_size())
    }
}

#[repr(C)]
struct BlockGroupDescriptorTable {
    /// Block ID of first block in the group's block bitmap.
    block_bitmap: u32,
    /// Block ID of the first block in the group's inode bitmap.
    inode_bitmap: u32,
    /// Block ID of the first block in the group's inode table.
    inode_table: u32,
    /// Total number of free blocks for the group.
    free_blocks_count: u16,
    /// Total number of free inodes for the group.
    free_inodes_count: u16,
    /// Total number of inodes allocated to directories.
    used_dirs_count: u16,
    _pad: u16,
    _reserved: [u8; 12],
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ext2Inode {
    /// Describes file type and access rights.
    mode: u16,
    /// User ID associated with file.
    uid: u16,
    /// Size of the file in bytes (lower 32 bit).
    size: u32,
    /// Unix timestamp of last access.
    atime: u32,
    /// Unix timestamp of creation.
    ctime: u32,
    /// Unix timestamp of last modification.
    mtime: u32,
    /// Unix timestamp of deletion.
    dtime: u32,
    /// Group ID associated with this file.
    gid: u16,
    /// Hard links to this inode.
    links_count: u16,
    /// Total number of 512-byte blocks reserved for this inode.
    blocks: u32,
    /// Various Flags for Inode access.
    flags: u32,
    /// OS dependent value.
    osd1: u32,
    /// First 13 entries are block ids of data blocks.
    /// 14th entry is to a block of block ids of further data blocks.
    /// 15th entry is to a block of block ids of block id lists.
    block: [u32; 15],
    /// File version.
    generation: u32,
    /// Extended file attributes.
    file_acl: u32,
    /// Upper 32 bits of file size.
    dir_acl: u32,
    /// File fragment location. Unused.
    faddr: u32,
    /// OS dependent value.
    osd2: [u8; 12],
}

impl Ext2Inode {
    fn is_file(&self) -> bool {
        (self.mode >> 12) == 0x8
    }

    fn is_directory(&self) -> bool {
        (self.mode >> 12) == 0x4
    }
}

/// Header describing an ext2 dentry, excluding the variable length name.
///
/// Should be 4 byte aligned in storage.
#[repr(C)]
struct LinkedDirectoryEntryHeader {
    /// Inode number to point to. 0 if unused.
    inode: u32,
    /// Displacement to next directory entry (or end of block).
    rec_len: u16,
    /// Name length.
    name_len: u8,
    /// File type
    file_type: u8,
    // Followed by 0-255 bytes of the file name
}

pub struct Ext2Fs {
    super_block: Arc<SuperBlock>,
    cache: Arc<Ext2InodeCache>,
}

impl Ext2Fs {
    fn parse_super_block() -> SuperBlock {
        let mut block_raw: UniqueArc<[u8; 256]> = UniqueArc::zeroed();

        block_cache().read(1024, block_raw.as_mut_slice());

        let cast: *const SuperBlock = (*block_raw).as_ptr().cast();

        assert_eq!(unsafe { (*cast).magic }, 0xEF53);

        unsafe { cast.read() }
    }

    pub fn new() -> Self {
        let super_block = Arc::new(Self::parse_super_block());
        Self {
            cache: Arc::new(Ext2InodeCache::new(super_block.clone())),
            super_block,
        }
    }

    fn lookup_node(&self, inode: u32) -> Option<Ext2Inode> {
        let mut block_buffer: UniqueArc<[u8; 1024]> = UniqueArc::zeroed();

        let group = (inode as u64 - 1) / (self.super_block.inodes_per_group as u64);
        let index = (inode as u64 - 1) % (self.super_block.inodes_per_group as u64);

        let first_inode_table_block = group * self.super_block.blocks_per_group as u64
            + self.super_block.inode_table_offset()
            + 1;

        let desired_inode_table_offset = first_inode_table_block * self.super_block.block_size()
            + (index * self.super_block.inode_size as u64);

        block_cache().read(
            desired_inode_table_offset as usize,
            block_buffer.as_mut_slice(),
        );

        let inode = (*block_buffer).as_ptr() as *const Ext2Inode;
        Some(unsafe { inode.read() })
    }
}

struct Ext2InodeCursor<'a> {
    inode: &'a Ext2Inode,
    /// Which block we're reading right now.
    block: u32,
    /// Offset within the 15 block top level list
    top_offset: u32,
    // Block offset within block 14's singly linked list
    l1_offset: u32,
    // Block offsets within block 15's doubly linked list
    l2_offsets: [u32; 2],
}

impl<'a> Ext2InodeCursor<'a> {
    pub fn new(inode: &'a Ext2Inode) -> Self {
        Self {
            inode,
            block: 0,
            top_offset: 0,
            l1_offset: 0,
            l2_offsets: [0; 2],
        }
    }

    fn jump_to(&mut self, block: u32) {
        const ASSUMED_BLOCK_SIZE: u32 = 1024;
        const BYTES_FOR_BLOCK: u32 = 4;
        const POINTERS_PER_BLOCK: u32 = ASSUMED_BLOCK_SIZE / BYTES_FOR_BLOCK;

        if block < 13 {
            self.top_offset = block;
            self.l1_offset = 0;
            self.l2_offsets = [0; 2];
        } else if block - 13 < POINTERS_PER_BLOCK {
            self.top_offset = 13;
            self.l1_offset = block - 13;
            self.l2_offsets = [0; 2];
        } else {
            let l2_index = block - POINTERS_PER_BLOCK - 13;
            self.top_offset = 14;
            self.l1_offset = 0;
            let upper_l2_offset = l2_index / POINTERS_PER_BLOCK;
            let lower_l2_offset = l2_index % POINTERS_PER_BLOCK;
            self.l2_offsets = [upper_l2_offset, lower_l2_offset];
        }
    }

    fn next_block(&mut self) {
        self.jump_to(self.block + 1);
    }

    fn get_current_block(&self) -> u32 {
        let mut buf = [0u8; 4];

        let top_level_block = self.inode.block[self.top_offset as usize];

        if self.top_offset < 13 {
            top_level_block
        } else if self.top_offset == 14 {
            block_cache().read(top_level_block as usize * 1024, &mut buf);
            u32::from_le_bytes(buf)
        } else {
            block_cache().read(top_level_block as usize * 1024, &mut buf);
            let next_level_block = u32::from_le_bytes(buf);
            block_cache().read(next_level_block as usize * 1024, &mut buf);
            u32::from_le_bytes(buf)
        }
    }

    fn read(&mut self, mut offset: u64, buf: &mut [u8]) -> usize {
        let mut have_read = 0;
        while have_read < buf.len() {
            let block_number = (offset / 1024) as u32;
            let block_offset = (offset % 1024) as u32;
            self.jump_to(block_number);

            let block = self.get_current_block();
            if block == 0 {
                return have_read;
            }

            let to_read_in_block =
                (1024 - block_offset).min(buf.len().saturating_sub(have_read) as u32);

            block_cache().read(
                (block * 1024 + block_offset) as usize,
                &mut buf[(have_read)..(have_read + to_read_in_block as usize)],
            );

            have_read += to_read_in_block as usize;
            offset += have_read as u64;
        }
        have_read
    }
}

struct Ext2InodeCache {
    super_block: Arc<SuperBlock>,
    cache: Mutex<List<Ext2InodeWrapper>>,
}

impl Arc<Ext2InodeCache> {
    fn lookup_or_create(&self, inode_number: u32) -> FsResult<Arc<Ext2InodeWrapper>> {
        let node = self.lookup(inode_number);

        if let Some(node) = node {
            Ok(node)
        } else {
            if !self.inode_exists(inode_number) {
                return Err(FsError::NoExist);
            }

            let mut block_buffer: UniqueArc<[u8; 1024]> = UniqueArc::zeroed();

            let group = (inode_number as u64 - 1) / (self.super_block.inodes_per_group as u64);
            let index = (inode_number as u64 - 1) % (self.super_block.inodes_per_group as u64);

            let first_inode_table_block = group * self.super_block.blocks_per_group as u64
                + self.super_block.inode_table_offset()
                + 1;

            let desired_inode_table_offset = first_inode_table_block
                * self.super_block.block_size()
                + (index * self.super_block.inode_size as u64);

            block_cache().read(
                desired_inode_table_offset as usize,
                block_buffer.as_mut_slice(),
            );

            let inode = (*block_buffer).as_ptr() as *const Ext2Inode;
            let node = unsafe { inode.read() };

            let wrapped = UniqueArc::new(Ext2InodeWrapper {
                super_block: self.super_block.clone(),
                inode_cache: self.clone(),
                number: inode_number,
                ext2_inode: node,
                links: ListLinks::new(),
            });

            Ok(self.insert(wrapped))
        }
    }
}

impl Ext2InodeCache {
    pub fn new(super_block: Arc<SuperBlock>) -> Self {
        Self {
            super_block,
            cache: Mutex::new(List::new()),
        }
    }

    fn lookup(&self, inode_number: u32) -> Option<Arc<Ext2InodeWrapper>> {
        let cache = self.cache.lock();
        let mut cursor = cache.cursor();

        while let Some(cached_inode) = cursor.get_arc()
            && cached_inode.number != inode_number
        {
            let _ = cursor.next();
        }

        cursor.get_arc()
    }

    fn insert(&self, inode: impl Into<ListArc<Ext2InodeWrapper, 0>>) -> Arc<Ext2InodeWrapper> {
        let list_arc = inode.into();
        let copy = list_arc.clone_arc();
        self.cache.lock().push_back(list_arc);

        copy
    }

    fn remove_inode(&self, inode_number: u32) {
        let mut cache = self.cache.lock();
        let mut cursor = cache.cursor_mut();

        while let Some(cached_inode) = cursor.get_arc()
            && cached_inode.number != inode_number
        {
            let _ = cursor.next();
        }

        if cursor.get().is_some() {
            cursor.remove();
        }
    }

    fn inode_exists(&self, inode: u32) -> bool {
        let group = (inode as u64 - 1) / (self.super_block.inodes_per_group as u64);
        let index = (inode as u64 - 1) % (self.super_block.inodes_per_group as u64);

        let first_inode_bitmap_block = group * self.super_block.blocks_per_group as u64
            + self.super_block.inode_bitmap_offset()
            + 1;

        let byte_desired = first_inode_bitmap_block * self.super_block.block_size() + index / 8;
        let bit_desired = index % 8;

        let mut byte = [0u8];
        block_cache().read(byte_desired as usize, &mut byte);

        ((byte[0] >> bit_desired) & 1) != 0
    }
}

struct Ext2InodeWrapper {
    super_block: Arc<SuperBlock>,
    inode_cache: Arc<Ext2InodeCache>,
    number: u32,
    ext2_inode: Ext2Inode,
    links: ListLinks,
}

impl_link!(Ext2InodeWrapper, 0 => links);

impl Ext2InodeWrapper {}

impl Drop for Ext2InodeWrapper {
    fn drop(&mut self) {
        self.inode_cache.remove_inode(self.number);
    }
}

impl InodeOperations for Ext2InodeWrapper {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut cursor = Ext2InodeCursor::new(&self.ext2_inode);

        Ok(cursor.read(offset, buffer))
    }

    fn list_directory(
        &self,
        list: &mut crate::utils::List<crate::utils::ListLinkWrapper<Arc<super::Inode>>>,
    ) -> FsResult<()> {
        if !self.ext2_inode.is_directory() {
            return Err(FsError::Unsupported);
        }

        let mut cursor = Ext2InodeCursor::new(&self.ext2_inode);

        let mut offset = 0;
        let mut dentry_header = [0u8; 8];

        let mut name_buffer = [0u8; 32];

        cursor.jump_to(0);
        while cursor.get_current_block() != 0 {
            while offset < 1024 {
                let _ = cursor.read(offset, &mut dentry_header);
                let overlay = dentry_header.as_ptr() as *const LinkedDirectoryEntryHeader;
                let inode_number = unsafe { (*overlay).inode };
                if inode_number != 0 {
                    let name_len = (unsafe { (*overlay).name_len } as usize).min(32);

                    cursor.read(offset + 8, &mut name_buffer[..name_len]);

                    let name = str::from_utf8(&name_buffer[..name_len]).unwrap();

                    let child = self.inode_cache.lookup_or_create(inode_number)?;

                    let fs_inode = Arc::new(Inode::new(child));

                    fs_inode.meta().set_name(name);

                    list.push_back(UniqueArc::new(ListLinkWrapper::new(fs_inode)).into());
                }
                offset += unsafe { (*overlay).rec_len as u64 };
            }

            cursor.next_block();
        }

        Ok(())
    }
}
