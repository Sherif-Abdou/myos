use crate::{
    impl_link,
    subsystem::{
        FsError, FsResult, Inode, InodeOperations,
        fs::ext2::{
            cache::Ext2InodeCache,
            cursor::{Ext2InodeCursor, Ext2InodeWriteCursor},
            raw::{Ext2Inode, LinkedDirectoryEntryHeader, SuperBlock},
        },
    },
    utils::{Arc, ListLinkWrapper, ListLinks, SpinLock, UniqueArc},
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
    pub(crate) links: ListLinks,
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

    fn write(&self, offset: u64, buffer: &[u8]) -> FsResult<()> {
        let mut write_cursor =
            Ext2InodeWriteCursor::new(self.number, &self.ext2_inode, &self.inode_cache);
        let _ = write_cursor.write(offset, buffer);

        Ok(())
    }

    fn list_directory(
        &self,
        list: &mut crate::utils::List<crate::utils::ListLinkWrapper<Arc<Inode>>>,
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
            while offset < self.super_block.block_size() {
                let _ = cursor.read(offset, &mut dentry_header);
                let overlay = dentry_header.as_ptr() as *const LinkedDirectoryEntryHeader;
                let inode_number = unsafe { (*overlay).inode };
                if inode_number != 0 {
                    let name_len = (unsafe { (*overlay).name_len } as usize).min(32);

                    cursor.read(offset + 8, &mut name_buffer[..name_len]);

                    let name = str::from_utf8(&name_buffer[..name_len]).unwrap();

                    let child = self.inode_cache.lookup_or_create(inode_number)?;

                    let file_size = *child.ext2_inode.size.lock() as u64;

                    let fs_inode = Arc::new(Inode::new(child));

                    fs_inode.meta().file_size = file_size;

                    fs_inode.meta().set_name(name);

                    list.push_back(UniqueArc::new(ListLinkWrapper::new(fs_inode)).into());
                }
                let rec_len = unsafe { (*overlay).rec_len as u64 };
                offset += rec_len;
            }

            cursor.next_block();
        }

        Ok(())
    }

    fn create_file(&self, name: &str) -> FsResult<()> {
        let mut write_cursor =
            Ext2InodeWriteCursor::new(self.number, &self.ext2_inode, &self.inode_cache);

        let new_inode_number = self.inode_cache.allocate_inode_number();

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

        write_cursor.append_dentry(new_inode_number, name);

        Ok(())
    }
}
