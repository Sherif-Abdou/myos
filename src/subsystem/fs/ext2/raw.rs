#[repr(C)]
pub struct SuperBlock {
    /// Total number of inodes, used + free, in the fs.
    pub(crate) inodes_count: u32,
    /// Total number of blocks, used + free, in the fs.
    pub(crate) blocks_count: u32,
    /// Total number of blocks reserved for super user.
    pub(crate) r_blocks_count: u32,
    /// Total number of free blocks.
    pub(crate) free_blocks_count: u32,
    /// Total number of free inodes.
    pub(crate) free_inodes_count: u32,
    /// ID of the first datablock.
    pub(crate) first_data_block: u32,
    /// log2(max block size) - 10
    pub(crate) log_block_size: u32,
    /// log2(max frag size) - 10
    pub(crate) log_frag_size: u32,
    /// Total number of blocks per group.
    pub(crate) blocks_per_group: u32,
    /// Total number of fragments per group.
    pub(crate) frags_per_group: u32,
    /// Totla number of inodes per group.
    pub(crate) inodes_per_group: u32,
    /// Unix time of last time mounted.
    pub(crate) mtime: u32,
    /// Unix time of last write.
    pub(crate) wtime: u32,
    /// How many times the file system was mounted before last verify.
    pub(crate) mnt_count: u16,
    /// Max times to mount before verification.
    pub(crate) max_mnt_count: u16,
    /// Ext2 identifcation magic number. 0xEF53.
    pub(crate) magic: u16,
    /// Indicates file system state.
    pub(crate) state: u16,
    /// What the driver should do if errors are detected.
    pub(crate) errors: u16,
    /// Minor revision level.
    pub(crate) minor_rev_level: u16,
    /// Unix time of last fs check.
    pub(crate) last_check: u32,
    /// Max Unix time interval between fs checks.
    pub(crate) check_interval: u32,
    /// Identifier of OS that created file system.
    pub(crate) creator_os: u32,
    /// Revision level value.
    pub(crate) rev_level: u32,
    /// Default user id for reserved blocks.
    pub(crate) def_resuid: u16,
    /// Default group id for reserved blocks.
    pub(crate) def_resgid: u16,
    /// First usable Inode index (usually 11).
    pub(crate) first_inode: u32,
    /// Inode size
    pub(crate) inode_size: u16,
    /// Block group number hosting this superblock structure.
    pub(crate) block_group_number: u16,
    /// Compatible features of this filesystem.
    ///
    /// FS can be loaded even if these features aren't supported.
    pub(crate) feature_compatible: u32,
    /// Incompatible features of this filesystem.
    ///
    /// FS can not be loaded if these features aren't supported.
    pub(crate) feature_incompatible: u32,
    /// Read-only compatible features of this filesystem.
    ///
    ///
    /// FS can only be ro mounted if these features aren't supported.
    pub(crate) feature_ro_compatible: u32,
    /// UUID of this FS
    pub(crate) uuid: [u8; 16],
    /// Name of this volume. Should be 0 terminated.
    pub(crate) volume_name: [u8; 16],
    /// Directory where this was last mounted. Should be 0 terminated.
    pub(crate) last_mounted: [u8; 64],
    /// Used to determine compression algorithm used.
    pub(crate) algo_bitmap: u32,
}

impl SuperBlock {
    pub fn block_size(&self) -> u64 {
        1024 << (self.log_block_size as u64)
    }

    pub fn group_count(&self) -> u64 {
        (self.blocks_count / self.blocks_per_group) as u64
    }

    pub fn super_block_offset(&self) -> u64 {
        0
    }

    pub fn group_descriptor_offset(&self) -> u64 {
        self.super_block_offset() + 1
    }

    pub fn group_descriptor_blocks(&self) -> u64 {
        (self.group_count() * 32).div_ceil(self.block_size())
    }

    pub fn block_bitmap_offset(&self) -> u64 {
        self.group_descriptor_offset() + self.group_descriptor_blocks()
    }

    pub fn block_bitmap_blocks(&self) -> u64 {
        (self.blocks_per_group as u64).div_ceil(self.block_size() * 8)
    }

    pub fn inode_bitmap_offset(&self) -> u64 {
        self.block_bitmap_offset() + self.block_bitmap_blocks()
    }

    pub fn inode_bitmap_blocks(&self) -> u64 {
        (self.inodes_per_group as u64).div_ceil(self.block_size() * 8)
    }

    pub fn inode_table_offset(&self) -> u64 {
        self.inode_bitmap_offset() + self.inode_bitmap_blocks()
    }

    pub fn inode_table_blocks(&self) -> u64 {
        (self.inodes_per_group as u64 * self.inode_size as u64).div_ceil(self.block_size())
    }
}

#[repr(C)]
pub struct BlockGroupDescriptorTable {
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
pub struct Ext2Inode {
    /// Describes file type and access rights.
    pub(crate) mode: u16,
    /// User ID associated with file.
    pub(crate) uid: u16,
    /// Size of the file in bytes (lower 32 bit).
    pub(crate) size: u32,
    /// Unix timestamp of last access.
    pub(crate) atime: u32,
    /// Unix timestamp of creation.
    pub(crate) ctime: u32,
    /// Unix timestamp of last modification.
    pub(crate) mtime: u32,
    /// Unix timestamp of deletion.
    pub(crate) dtime: u32,
    /// Group ID associated with this file.
    pub(crate) gid: u16,
    /// Hard links to this inode.
    pub(crate) links_count: u16,
    /// Total number of 512-byte blocks reserved for this inode.
    pub(crate) blocks: u32,
    /// Various Flags for Inode access.
    pub(crate) flags: u32,
    /// OS dependent value.
    pub(crate) osd1: u32,
    /// First 13 entries are block ids of data blocks.
    /// 14th entry is to a block of block ids of further data blocks.
    /// 15th entry is to a block of block ids of block id lists.
    pub(crate) block: [u32; 15],
    /// File version.
    pub(crate) generation: u32,
    /// Extended file attributes.
    pub(crate) file_acl: u32,
    /// Upper 32 bits of file size.
    pub(crate) dir_acl: u32,
    /// File fragment location. Unused.
    pub(crate) faddr: u32,
    /// OS dependent value.
    pub(crate) osd2: [u8; 12],
}

impl Ext2Inode {
    pub fn is_file(&self) -> bool {
        (self.mode >> 12) == 0x8
    }

    pub fn is_directory(&self) -> bool {
        (self.mode >> 12) == 0x4
    }
}

/// Header describing an ext2 dentry, excluding the variable length name.
///
/// Should be 4 byte aligned in storage.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedDirectoryEntryHeader {
    /// Inode number to point to. 0 if unused.
    pub(crate) inode: u32,
    /// Displacement to next directory entry (or end of block).
    pub(crate) rec_len: u16,
    /// Name length.
    pub(crate) name_len: u8,
    /// File type
    pub(crate) file_type: u8,
    // Followed by 0-255 bytes of the file name
}
