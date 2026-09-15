use core::mem::MaybeUninit;

use crate::{
    impl_link,
    sched::Mutex,
    subsystem::{
        FsError, FsResult, Inode, InodeDirectoryEntry, InodeOperations, block_cache,
        fs::ext2::{
            cache::Ext2InodeCache,
            cursor::{Ext2InodeCursor, Ext2InodeWriteCursor},
            raw::{Ext2Inode, LinkedDirectoryEntryHeader, SuperBlock},
        },
    },
    utils::{Arc, KString, List, ListLinks, SpinLock, UniqueArc, copy_to_uninit},
};

pub struct Ext2Meta {
    pub(crate) mode: SpinLock<u16>,
    pub(crate) size: SpinLock<u32>,
    pub(crate) block: SpinLock<[u32; 15]>,
    pub(crate) links_count: SpinLock<u16>,
}

impl Ext2Meta {
    pub fn from_ext2_inode(ext2_inode: &Ext2Inode) -> Self {
        Self {
            mode: SpinLock::new(ext2_inode.mode),
            size: SpinLock::new(ext2_inode.size),
            block: SpinLock::new(ext2_inode.block),
            links_count: SpinLock::new(ext2_inode.links_count),
        }
    }

    pub fn is_file(&self) -> bool {
        (*self.mode.lock() >> 12) == 0x8
    }

    pub fn is_directory(&self) -> bool {
        (*self.mode.lock() >> 12) == 0x4
    }
}

pub struct Ext2InodeWrapper {
    pub(crate) super_block: Arc<SuperBlock>,
    pub(crate) inode_cache: Arc<Ext2InodeCache>,
    pub(crate) number: u32,
    pub(crate) ext2_inode: Ext2Meta,
    pub(crate) io_lock: Mutex<()>,
    pub(crate) links: ListLinks,
}

impl Ext2InodeWrapper {
    /// Walks a linked list block
    fn walk_block<F: Fn(u32)>(block: u32, func: F) {
        let mut buf = [0u8; 64];

        for i in 0..(1024 / buf.len()) {
            block_cache().read(block as usize * 1024 + i * buf.len(), &mut buf);

            for subset in 0..(buf.len() / 4) {
                let block =
                    u32::from_le_bytes(buf[4 * subset..4 * (subset + 1)].try_into().unwrap());

                func(block);
            }
        }
    }
}

impl Drop for Ext2InodeWrapper {
    fn drop(&mut self) {
        if *self.ext2_inode.links_count.lock() == 0 {
            let blocks = *self.ext2_inode.block.lock();
            for block in blocks.iter().take(12) {
                if *block != 0 {
                    self.inode_cache.free_data_block(*block);
                }
            }

            let single_linked_block = blocks[12];
            if single_linked_block != 0 {
                Self::walk_block(single_linked_block, |block| {
                    if block != 0 {
                        self.inode_cache.free_data_block(block);
                    }
                });
                self.inode_cache.free_data_block(single_linked_block);
            }

            let double_linked_block = blocks[13];
            if double_linked_block != 0 {
                Self::walk_block(double_linked_block, |single_linked_block| {
                    if single_linked_block != 0 {
                        Self::walk_block(single_linked_block, |block| {
                            if block != 0 {
                                self.inode_cache.free_data_block(block);
                            }
                        });
                        self.inode_cache.free_data_block(single_linked_block);
                    }
                });
                self.inode_cache.free_data_block(double_linked_block);
            }

            let triple_linked_block = blocks[14];
            if triple_linked_block != 0 {
                Self::walk_block(triple_linked_block, |double_linked_block| {
                    if double_linked_block != 0 {
                        Self::walk_block(double_linked_block, |single_linked_block| {
                            if single_linked_block != 0 {
                                Self::walk_block(single_linked_block, |block| {
                                    if block != 0 {
                                        self.inode_cache.free_data_block(block);
                                    }
                                });
                                self.inode_cache.free_data_block(single_linked_block);
                            }
                        });
                        self.inode_cache.free_data_block(double_linked_block);
                    }
                });
                self.inode_cache.free_data_block(triple_linked_block);
            }

            self.inode_cache.remove_inode(self.number);
            self.inode_cache.free_inode(self.number);
        }
    }
}

impl_link!(Ext2InodeWrapper, 0 => links);

impl Ext2InodeWrapper {}

impl InodeOperations for Ext2InodeWrapper {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let _lock = self.io_lock.lock();

        let mut cursor = Ext2InodeCursor::new(&self.ext2_inode);

        cursor.read(offset, buffer)
    }

    fn write(&self, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let _lock = self.io_lock.lock();

        let mut write_cursor =
            Ext2InodeWriteCursor::new(self.number, &self.ext2_inode, &self.inode_cache);
        let written = write_cursor.write(offset, buffer);

        Ok(written)
    }

    fn list_directory(&self, list: &mut crate::utils::List<InodeDirectoryEntry>) -> FsResult<()> {
        if !self.ext2_inode.is_directory() {
            return Err(FsError::Unsupported);
        }

        let _lock = self.io_lock.lock();

        let file_size = *self.ext2_inode.size.lock() as u64;
        let mut file_offset = 0;
        let mut block_offset = 0;

        let mut dentry_header: MaybeUninit<LinkedDirectoryEntryHeader> = MaybeUninit::uninit();

        let mut cursor = Ext2InodeCursor::new(&self.ext2_inode);
        // TOOD: Support full length file names.
        let mut name_buffer = [0u8; 256];

        let mut active_block = cursor.get_current_block();
        while active_block != 0 && file_offset < file_size {
            while block_offset < 1024 && file_offset < file_size {
                let mut dentry_buffer = [0u8; core::mem::size_of::<LinkedDirectoryEntryHeader>()];

                cursor.read_exact(file_offset, &mut dentry_buffer)?;

                copy_to_uninit(&mut dentry_header, &dentry_buffer);

                let dentry_header = unsafe { dentry_header.assume_init_ref() };

                if dentry_header.rec_len <= 8
                    || !dentry_header.rec_len.is_multiple_of(4)
                    || block_offset + dentry_header.rec_len as usize > 1024
                    || dentry_header.name_len as u16 > dentry_header.rec_len - 8
                {
                    return Err(FsError::BadMeta);
                }

                let inode_number = dentry_header.inode;

                if inode_number != 0 {
                    let ext2_inode = self.inode_cache.lookup_or_create(inode_number)?;

                    let ext2_file_size = *ext2_inode.ext2_inode.size.lock();

                    let inode = Arc::new(Inode::new(ext2_inode));

                    let name_len = (dentry_header.name_len as usize).min(name_buffer.len());

                    cursor.read_exact(file_offset + 8, &mut name_buffer[..name_len])?;

                    let mut name = KString::new();
                    name.extend_from_slice(&name_buffer[..name_len]);

                    inode.meta().file_size = ext2_file_size as u64;

                    list.push_back(UniqueArc::new(InodeDirectoryEntry::new(name, inode)).into());
                }

                file_offset += dentry_header.rec_len as u64;
                block_offset += dentry_header.rec_len as usize;
            }

            cursor.next_block();
            active_block = cursor.get_current_block();
            block_offset = 0;
        }

        Ok(())
    }

    fn create_file(&self, name: &str) -> FsResult<()> {
        let mut write_cursor =
            Ext2InodeWriteCursor::new(self.number, &self.ext2_inode, &self.inode_cache);

        let new_inode_number = self.inode_cache.allocate_inode_number();
        if new_inode_number == 0 {
            return Err(FsError::NoSpace);
        }

        let inode = Ext2Inode {
            mode: (0x8 << 12) | (0o777),
            uid: 0,
            size: 0,
            atime: 0,
            ctime: 0,
            mtime: 0,
            dtime: 0,
            gid: 0,
            links_count: 1,
            blocks: 2,
            flags: 0,
            osd1: 0,
            block: [0; _],
            generation: 0,
            file_acl: 0,
            dir_acl: 0,
            faddr: 0,
            osd2: [0; _],
        };

        self.inode_cache.write_node(new_inode_number, &inode);

        write_cursor.append_dentry(new_inode_number, name)?;

        Ok(())
    }

    fn create_directory(&self, name: &str) -> FsResult<()> {
        let mut write_cursor =
            Ext2InodeWriteCursor::new(self.number, &self.ext2_inode, &self.inode_cache);

        let new_inode_number = self.inode_cache.allocate_inode_number();
        if new_inode_number == 0 {
            return Err(FsError::NoSpace);
        }

        let inode = Ext2Inode {
            mode: (0x4 << 12) | (0o666),
            uid: 0,
            size: 0,
            atime: 0,
            ctime: 0,
            mtime: 0,
            dtime: 0,
            gid: 0,
            links_count: 2,
            blocks: 2,
            flags: 0,
            osd1: 0,
            block: [0; _],
            generation: 0,
            file_acl: 0,
            dir_acl: 0,
            faddr: 0,
            osd2: [0; _],
        };

        self.inode_cache.write_node(new_inode_number, &inode);

        write_cursor.append_dentry(new_inode_number, name)?;

        let inode = self.inode_cache.lookup_or_create(new_inode_number)?;

        let mut write_cursor =
            Ext2InodeWriteCursor::new(inode.number, &inode.ext2_inode, &inode.inode_cache);

        write_cursor.append_dentry(new_inode_number, ".")?;
        write_cursor.append_dentry(self.number, "..")?;

        *write_cursor.meta().links_count.lock() += 1;

        self.inode_cache.modify_node(self.number, |inode| {
            inode.links_count += 1;
        });

        Ok(())
    }

    fn remove_file(&self, name: &str) -> FsResult<()> {
        let _io_lock = self.io_lock.lock();

        let mut cursor =
            Ext2InodeWriteCursor::new(self.number, &self.ext2_inode, &self.inode_cache);

        let target_child_dentry = cursor.find_dentry_with_name(name)?;

        // Update the previous link to jump over the dentry.
        if !target_child_dentry.is_dentry_start_of_block() {
            let rec_len = cursor
                .read_dentry_header(target_child_dentry.dentry_offset())?
                .rec_len;

            cursor.rmw_dentry_header(
                target_child_dentry
                    .previous_dentry_offset()
                    .ok_or(FsError::BadMeta)?,
                |mut dentry| {
                    dentry.rec_len += rec_len;

                    dentry
                },
            )?;
        } else {
            // Mark inode = 0
            cursor.rmw_dentry_header(target_child_dentry.dentry_offset(), |mut dentry| {
                dentry.inode = 0;

                dentry
            })?;
        }

        *self.ext2_inode.links_count.lock() -= 1;
        self.inode_cache.modify_node(self.number, |inode| {
            inode.links_count -= 1;
        });

        Ok(())
    }

    fn remove_directory(&self, name: &str) -> FsResult<()> {
        let _io_lock = self.io_lock.lock();

        let mut cursor =
            Ext2InodeWriteCursor::new(self.number, &self.ext2_inode, &self.inode_cache);

        let target_child_dentry = cursor.find_dentry_with_name(name)?;

        let child_inode = self
            .inode_cache
            .lookup_or_create(target_child_dentry.dentry_inode())?;

        let mut list = List::new();
        child_inode.list_directory(&mut list)?;

        // Assume the invariant that . and .. are the first two files.
        //
        // TODO: Perhaps verify this eventually.
        if list.len() > 2 {
            return Err(FsError::NotEmpty);
        }

        // Update the previous link to jump over the dentry.
        if !target_child_dentry.is_dentry_start_of_block() {
            let rec_len = cursor
                .read_dentry_header(target_child_dentry.dentry_offset())?
                .rec_len;

            cursor.rmw_dentry_header(
                target_child_dentry
                    .previous_dentry_offset()
                    .ok_or(FsError::BadMeta)?,
                |mut dentry| {
                    dentry.rec_len += rec_len;

                    dentry
                },
            )?;
        } else {
            // Mark inode = 0
            cursor.rmw_dentry_header(target_child_dentry.dentry_offset(), |mut dentry| {
                dentry.inode = 0;

                dentry
            })?;
        }

        *self.ext2_inode.links_count.lock() -= 1;
        self.inode_cache.modify_node(self.number, |inode| {
            inode.links_count -= 1;
        });

        Ok(())
    }

    fn truncate(&self, desired_size: usize) -> FsResult<()> {
        let _iolock = self.io_lock.lock();

        if desired_size > u32::MAX as usize {
            return Err(FsError::InvalidArgument);
        }

        *self.ext2_inode.size.lock() = desired_size as u32;
        self.inode_cache.modify_node(self.number, |inode| {
            inode.size = desired_size as u32;
        });

        Ok(())
    }
}
