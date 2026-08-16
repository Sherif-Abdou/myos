use crate::{
    sched::Mutex,
    subsystem::{
        FsError, FsResult, block_cache,
        fs::ext2::{
            inode::{Ext2InodeWrapper, Ext2Meta},
            raw::{Ext2Inode, SuperBlock},
        },
    },
    utils::{Arc, List, ListArc, ListLinks, UniqueArc},
};

pub struct Ext2InodeCache {
    super_block: Arc<SuperBlock>,
    cache: Mutex<List<Ext2InodeWrapper>>,
    bitmask_lock: Mutex<()>,
}

impl Arc<Ext2InodeCache> {
    /// Look up the inode in the cache, or load it from the FS if not cached.
    pub fn lookup_or_create(&self, inode_number: u32) -> FsResult<Arc<Ext2InodeWrapper>> {
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
                ext2_inode: Ext2Meta::from_ext2_inode(&node),
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
            bitmask_lock: Mutex::new(()),
        }
    }

    /// Looks up an inode in the cache.
    pub fn lookup(&self, inode_number: u32) -> Option<Arc<Ext2InodeWrapper>> {
        let cache = self.cache.lock();
        let mut cursor = cache.cursor();

        while let Some(cached_inode) = cursor.get_arc()
            && cached_inode.number != inode_number
        {
            let _ = cursor.next();
        }

        cursor.get_arc()
    }

    /// Inserts a loaded inode into the cache.
    pub fn insert(&self, inode: impl Into<ListArc<Ext2InodeWrapper, 0>>) -> Arc<Ext2InodeWrapper> {
        let list_arc = inode.into();
        let copy = list_arc.clone_arc();
        self.cache.lock().push_back(list_arc);

        copy
    }

    /// Removes an inode from the cache.
    pub fn remove_inode(&self, inode_number: u32) {
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

    /// Checks if the inode exists within the file system.
    pub fn inode_exists(&self, inode: u32) -> bool {
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

    /// Finds and reserves a data block to be used for a given inode.
    pub fn allocate_data_block(&self, inode_number: u32) -> u32 {
        let _allocation_lock = self.bitmask_lock.lock();

        fn scan_and_reserve_first_open(block: &mut [u8]) -> Option<u32> {
            for (byte_num, byte) in block.iter_mut().enumerate() {
                if *byte != 0xFF {
                    let index = (!*byte).lowest_one().unwrap();

                    *byte |= 1 << index;

                    return Some(index + byte_num as u32 * 8);
                }
            }

            None
        }

        let mut block_buffer: UniqueArc<[u8; 1024]> = UniqueArc::zeroed();

        let group = (inode_number as u64 - 1) / (self.super_block.inodes_per_group as u64);

        let mut rolled_over = false;
        let mut target_group = group;

        while !(rolled_over && target_group == group) {
            let block_bitmap_first_block = self.super_block.block_bitmap_offset();
            let block_bitmap_num_blocks = self.super_block.block_bitmap_blocks();

            for i in 0..block_bitmap_num_blocks {
                let current_bitmap_block = target_group * self.super_block.blocks_per_group as u64
                    + block_bitmap_first_block
                    + i
                    + 1;
                block_cache().read(
                    (current_bitmap_block * self.super_block.block_size()) as usize,
                    &mut block_buffer[..],
                );

                if let Some(first_open) = scan_and_reserve_first_open(block_buffer.as_mut_slice()) {
                    let block_number =
                        (target_group * self.super_block.block_size() * 8) as u32 + first_open;

                    block_cache().write(
                        (current_bitmap_block * self.super_block.block_size()) as usize,
                        &block_buffer[..],
                    );

                    return block_number + self.super_block.first_data_block;
                }
            }

            target_group += 1;
            if target_group >= self.super_block.group_count() {
                target_group = 0;
                rolled_over = true;
            }
        }

        0
    }

    /// Finds and allocates the next available inode. Returns that inode's number.
    pub fn allocate_inode_number(&self) -> u32 {
        let _allocation_lock = self.bitmask_lock.lock();

        fn scan_and_reserve_first_open(block: &mut [u8]) -> Option<u32> {
            for (byte_num, byte) in block.iter_mut().enumerate() {
                if *byte != 0xFF {
                    let index = (!*byte).lowest_one().unwrap();

                    *byte |= 1 << index;

                    return Some(index + byte_num as u32 * 8);
                }
            }

            None
        }

        let mut block_buffer: UniqueArc<[u8; 1024]> = UniqueArc::zeroed();

        let group = 0;

        let mut rolled_over = false;
        let mut target_group = group;

        while !(rolled_over && target_group == group) {
            let inode_bitmap_first_block = self.super_block.inode_bitmap_offset();
            let inode_bitmap_num_blocks = self.super_block.inode_bitmap_blocks();

            for i in 0..inode_bitmap_num_blocks {
                let current_bitmap_block = target_group * self.super_block.blocks_per_group as u64
                    + inode_bitmap_first_block
                    + i
                    + 1;
                block_cache().read(
                    (current_bitmap_block * self.super_block.block_size()) as usize,
                    &mut block_buffer[..],
                );

                if let Some(first_open) = scan_and_reserve_first_open(block_buffer.as_mut_slice()) {
                    let inode_number =
                        (target_group * self.super_block.block_size() * 8) as u32 + first_open + 1;

                    block_cache().write(
                        (current_bitmap_block * self.super_block.block_size()) as usize,
                        &block_buffer[..],
                    );

                    return inode_number;
                }
            }

            target_group += 1;
            if target_group >= self.super_block.group_count() {
                target_group = 0;
                rolled_over = true;
            }
        }

        0
    }

    /// Writes the inode reference to the inode number on disk.
    pub fn write_node(&self, inode_number: u32, inode: &Ext2Inode) {
        let group = (inode_number as u64 - 1) / (self.super_block.inodes_per_group as u64);
        let index = (inode_number as u64 - 1) % (self.super_block.inodes_per_group as u64);

        let first_inode_table_block = group * self.super_block.blocks_per_group as u64
            + self.super_block.inode_table_offset()
            + 1;

        let desired_inode_table_offset = first_inode_table_block * self.super_block.block_size()
            + (index * self.super_block.inode_size as u64);

        let inode_slice = unsafe {
            core::slice::from_raw_parts((&raw const *inode).cast::<u8>(), size_of_val(inode))
        };

        block_cache().write(desired_inode_table_offset as usize, inode_slice);
    }

    /// Modifies the inode reference to the inode number on disk.
    pub fn modify_node<R, F: FnOnce(&mut Ext2Inode) -> R>(&self, inode_number: u32, func: F) -> R {
        let mut block_buffer: UniqueArc<[u8; 128]> = UniqueArc::zeroed();

        let group = (inode_number as u64 - 1) / (self.super_block.inodes_per_group as u64);
        let index = (inode_number as u64 - 1) % (self.super_block.inodes_per_group as u64);

        let first_inode_table_block = group * self.super_block.blocks_per_group as u64
            + self.super_block.inode_table_offset()
            + 1;

        let desired_inode_table_offset = first_inode_table_block * self.super_block.block_size()
            + (index * self.super_block.inode_size as u64);

        block_cache().read(desired_inode_table_offset as usize, &mut block_buffer[..]);

        let node = unsafe {
            (block_buffer.as_mut_ptr() as *mut Ext2Inode)
                .cast::<Ext2Inode>()
                .as_mut()
                .unwrap()
        };

        let ret = func(node);

        block_cache().write(desired_inode_table_offset as usize, &block_buffer[..]);

        ret
    }
}
