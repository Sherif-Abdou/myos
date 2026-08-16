use alloc::slice;

use crate::{
    allocators::align_up,
    subsystem::{
        block_cache,
        fs::ext2::{cache::Ext2InodeCache, inode::Ext2Meta, raw::LinkedDirectoryEntryHeader},
    },
};

pub struct Ext2InodeWriteCursor<'a> {
    inode_number: u32,
    inode: &'a Ext2Meta,
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
    pub fn new(inode_number: u32, inode: &'a Ext2Meta, cache: &'a Ext2InodeCache) -> Self {
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

        self.block = block;

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
        } else if self.top_offset == 13 {
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
        } else if self.top_offset == 13 {
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

    pub fn write(&mut self, offset: u64, buf: &[u8]) -> usize {
        let mut current_offset = offset;
        let mut have_written = 0;
        while have_written < buf.len() {
            let block_number = (current_offset / 1024) as u32;
            let block_offset = (current_offset % 1024) as u32;
            self.jump_to(block_number);

            let mut block = self.get_current_block();
            if block == 0 {
                block = self.allocate_for_current_block();

                // If we failed to allocate, stop writing.
                if block == 0 {
                    break;
                }
            }

            let to_write_in_block =
                (1024 - block_offset).min(buf.len().saturating_sub(have_written) as u32);

            block_cache().write(
                (block * 1024 + block_offset) as usize,
                &buf[(have_written)..(have_written + to_write_in_block as usize)],
            );

            have_written += to_write_in_block as usize;
            current_offset += to_write_in_block as u64;
        }

        if offset + have_written as u64 > *self.inode.size.lock() as u64 {
            let new_size = offset as u32 + have_written as u32;
            *self.inode.size.lock() = new_size;
            self.cache.modify_node(self.inode_number, |inode| {
                inode.size = new_size;
            });
        }

        have_written
    }

    pub fn append_dentry(&mut self, child_inode_number: u32, name: &str) {
        let mut cursor = Ext2InodeCursor::new(self.inode);

        let mut offset = 0;
        let mut latest_rec_len = 0u64;
        let mut dentry_header = [0u8; 8];

        // Traverse the directory entry linked list to find the last node.
        cursor.jump_to(0);
        while cursor.get_current_block() != 0 {
            let mut local_block_offset = 0;
            while local_block_offset < 1024 {
                let _ = cursor.read(offset, &mut dentry_header);
                let overlay = dentry_header.as_ptr() as *const LinkedDirectoryEntryHeader;
                latest_rec_len = unsafe { (*overlay).rec_len as u64 };
                offset += latest_rec_len;
                local_block_offset += latest_rec_len;
            }

            cursor.next_block();
        }

        let start_of_last_header = offset.saturating_sub(latest_rec_len);
        let within_block_offset = start_of_last_header % 1024;

        let overlay = dentry_header.as_mut_ptr() as *mut LinkedDirectoryEntryHeader;

        let current_name_len = unsafe { (*overlay).name_len };

        let new_name_len = name.len();

        let necessary_size = new_name_len + 8;
        // Headers are expected to be four byte aligned.
        let next_available_offset = align_up(
            within_block_offset as usize + current_name_len as usize + 8,
            4,
        ) as u64;

        // We can append the dentry to this block.
        let next_available_start;
        let rec_len;
        if next_available_offset + necessary_size as u64 <= 1024 {
            next_available_start = align_up(
                start_of_last_header as usize + current_name_len as usize + 8,
                4,
            ) as u64;
            rec_len = (1024 - next_available_start) as u16;

            let updated_prev_rec_len = (next_available_start - within_block_offset) as u16;
            self.write(
                start_of_last_header + 4,
                &updated_prev_rec_len.to_le_bytes(),
            );
        } else {
            // Append to start of a new block.
            next_available_start = ((start_of_last_header / 1024) + 1) * 1024;
            rec_len = 1024;
        }

        let new_header = LinkedDirectoryEntryHeader {
            inode: child_inode_number,
            rec_len,
            name_len: new_name_len as u8,
            file_type: 0,
        };

        let new_header_buffer =
            unsafe { slice::from_raw_parts((&raw const new_header).cast::<u8>(), 8) };

        self.write(next_available_start, new_header_buffer);
        self.write(next_available_start + 8, name.as_bytes());
    }
}

pub struct Ext2InodeCursor<'a> {
    inode: &'a Ext2Meta,
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
    const BLOCK_SIZE: usize = 1024;

    pub fn new(inode: &'a Ext2Meta) -> Self {
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

        self.block = block;

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
        } else if self.top_offset == 13 {
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
            offset += to_read_in_block as u64;
        }
        have_read
    }
}
