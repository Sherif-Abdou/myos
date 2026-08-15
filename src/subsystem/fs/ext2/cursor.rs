use crate::subsystem::{
    block_cache,
    fs::ext2::{cache::Ext2InodeCache, inode::Ext2InodeMeta},
};

pub struct Ext2InodeWriteCursor<'a> {
    inode_number: u32,
    inode: &'a Ext2InodeMeta,
    /// Inode cache used for data block allocation.
    cache: &'a Ext2InodeCache,
    /// Which block we're reading right now.
    block: u32,
    /// Offset within the 15 block top level list
    top_offset: u32,
    // Block offset within block 14's singly linked list
    l1_offset: u32,
    // Block offsets within block 15's doubly linked list
    l2_offsets: [u32; 2],
}

impl<'a> Ext2InodeWriteCursor<'a> {
    pub fn new(inode_number: u32, inode: &'a Ext2InodeMeta, cache: &'a Ext2InodeCache) -> Self {
        Self {
            inode_number,
            inode,
            cache,
            block: 0,
            top_offset: 0,
            l1_offset: 0,
            l2_offsets: [0; 2],
        }
    }

    /// Jumps the cursor to the start of the blockth block.
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

    /// Moves the cursor to the start of the next data block in the file.
    fn next_block(&mut self) {
        self.jump_to(self.block + 1);
    }

    /// Gets the data block this cursor is currently pointed at.
    fn get_current_block(&self) -> u32 {
        let mut buf = [0u8; 4];

        let top_level_block = self.inode.block.lock()[self.top_offset as usize];

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

    /// Allocates a free data block and updates the inode to point to it.
    fn allocate_for_current_block(&mut self) -> u32 {
        let allocated = self.cache.allocate_data_block(self.inode_number);

        let mut buf = [0u8; 4];

        if self.top_offset < 13 {
            // Update the block pointer directly in the inode, then flush
            self.inode.block.lock()[self.top_offset as usize] = allocated;
            self.cache.modify_node(self.inode_number, |inode| {
                inode.block = *self.inode.block.lock();
            });
        } else if self.top_offset == 14 {
            // Fetch the offset within the linked list to update
            let single_linked_block = self.inode.block.lock()[self.top_offset as usize];
            let offset_within = self.l1_offset * 4;

            // Update that block pointer only.
            block_cache().write(
                (single_linked_block * 1024 + offset_within) as usize,
                &allocated.to_le_bytes(),
            );
        } else {
            // Find which linked list to look into
            let double_linked_block = self.inode.block.lock()[self.top_offset as usize];
            let offset_within_double = self.l2_offsets[0] * 4;
            block_cache().read(
                (double_linked_block * 1024 + offset_within_double) as usize,
                &mut buf,
            );

            // Update the appropriate linked list.
            let single_linked_block = u32::from_le_bytes(buf);
            let offset_within_single = self.l2_offsets[1] * 4;
            block_cache().write(
                (single_linked_block * 1024 + offset_within_single) as usize,
                &allocated.to_le_bytes(),
            );
        }
        allocated
    }

    pub fn write(&mut self, mut offset: u64, buf: &[u8]) -> usize {
        let mut have_written = 0;
        while have_written < buf.len() {
            let block_number = (offset / 1024) as u32;
            let block_offset = (offset % 1024) as u32;
            self.jump_to(block_number);

            let mut block = self.get_current_block();
            if block == 0 {
                block = self.allocate_for_current_block();
            }

            let to_read_in_block =
                (1024 - block_offset).min(buf.len().saturating_sub(have_written) as u32);

            block_cache().write(
                (block * 1024 + block_offset) as usize,
                &buf[(have_written)..(have_written + to_read_in_block as usize)],
            );

            have_written += to_read_in_block as usize;
            offset += have_written as u64;
        }
        have_written
    }
}

pub struct Ext2InodeCursor<'a> {
    inode: &'a Ext2InodeMeta,
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
    pub fn new(inode: &'a Ext2InodeMeta) -> Self {
        Self {
            inode,
            block: 0,
            top_offset: 0,
            l1_offset: 0,
            l2_offsets: [0; 2],
        }
    }

    /// Jumps the cursor to the start of the blockth block.
    pub fn jump_to(&mut self, block: u32) {
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

    /// Moves the cursor to the start of the next data block in the file.
    pub fn next_block(&mut self) {
        self.jump_to(self.block + 1);
    }

    /// Gets the data block this cursor is currently pointed at.
    pub fn get_current_block(&self) -> u32 {
        let mut buf = [0u8; 4];

        let top_level_block = self.inode.block.lock()[self.top_offset as usize];

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

    pub fn read(&mut self, mut offset: u64, buf: &mut [u8]) -> usize {
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
