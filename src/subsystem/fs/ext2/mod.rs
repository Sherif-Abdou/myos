//! Revision 1 Ext2 FS support (no features supported).

use crate::{
    subsystem::{
        FileSystem, FsError, Inode, block_cache,
        fs::ext2::{
            cache::Ext2InodeCache,
            raw::{Ext2Inode, SuperBlock},
        },
    },
    utils::{Arc, OnceSpinLock, UniqueArc},
};

mod cache;
mod cursor;
mod inode;
mod raw;

pub static EXT2_FS: OnceSpinLock<Ext2Fs> = OnceSpinLock::new();

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

    fn create(&self, path: &str) -> super::FsResult<Arc<Inode>> {
        let parts = path.split("/");
        let num_parts = path.chars().filter(|c| *c == '/').count();

        let mut current = self.root();
        for part in parts.take(num_parts - 1) {
            if part.is_empty() {
                continue;
            }

            let potential_child = current.list_directory(|list| {
                let mut cursor = list.cursor();
                while cursor
                    .get()
                    .is_some_and(|child| child.meta().name() != part)
                {
                    let _ = cursor.next();
                }

                cursor.get_arc()
            })?;

            let child = potential_child.ok_or(FsError::NoExist)?;
            current = (*child).clone();
        }

        let child_to_create = path.split('/').nth(num_parts).unwrap();
        current.create_file(child_to_create)?;

        current.list_directory(|list| {
            let mut cursor = list.cursor();
            while cursor
                .get()
                .is_some_and(|child| child.meta().name() != child_to_create)
            {
                let _ = cursor.next();
            }

            cursor
                .get_arc()
                .map(|wrapper| (*wrapper).clone())
                .ok_or(FsError::NoExist)
        })?
    }

    fn open(&self, path: &str) -> super::FsResult<Arc<Inode>> {
        let parts = path.split("/");

        let mut current = self.root();
        for part in parts {
            if part.is_empty() {
                continue;
            }

            let potential_child = current.list_directory(|list| {
                let mut cursor = list.cursor();
                while cursor
                    .get()
                    .is_some_and(|child| child.meta().name() != part)
                {
                    let _ = cursor.next();
                }

                cursor.get_arc()
            })?;

            let child = potential_child.ok_or(FsError::NoExist)?;
            current = (*child).clone();
        }
        Ok(current)
    }
}
