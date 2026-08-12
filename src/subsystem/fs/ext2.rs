//! Revision 1 Ext2 FS support.

use crate::{printk, subsystem::block_cache, utils::UniqueArc};

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
    /// Total number of blocks reserved for this inode.
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
    /// OS depdendent value.
    osd2: [u8; 12],
}

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

pub fn read_super_block() {
    let mut buf: UniqueArc<[u8; 1024]> = UniqueArc::zeroed();

    block_cache().read(1024, &mut buf[..]);

    let start_lower = buf[0x38] as u16;
    let start_upper = buf[0x39] as u16;

    printk!("Magic: {:x}\n", start_lower | (start_upper << 8));
}
