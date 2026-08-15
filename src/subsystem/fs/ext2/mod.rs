//! Revision 1 Ext2 FS support (no features supported).

use crate::{
    subsystem::{
        FileSystem, Inode, block_cache,
        fs::ext2::{
            cache::Ext2InodeCache,
            raw::{Ext2Inode, SuperBlock},
        },
    },
    utils::{Arc, UniqueArc},
};

mod cache;
mod cursor;
mod inode;
mod raw;

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

    /// Reads the ext2 inode from the fs.
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

impl FileSystem for Ext2Fs {
    fn root(&self) -> Arc<Inode> {
        let node = self.cache.lookup_or_create(2).unwrap();

        Arc::new(Inode::new(node))
    }
}
