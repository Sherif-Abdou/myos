use crate::{
    impl_link, sched::Mutex, subsystem::{
        FileSystem, FsResult,
        fs::{Inode, InodeOperations},
    }, utils::{Arc, List, ListArc, ListLinkWrapper, ListLinks, SpinLock, UniqueArc},
};

use super::FsError;

const TMPFS_BLOCK_SIZE: usize = 4096;

pub struct FileBlockContents {
    /// Raw contents of the block.
    block: [u8; TMPFS_BLOCK_SIZE],
    /// Used size within this block.
    size: usize,
}

pub struct FileBlock {
    contents: SpinLock<FileBlockContents>,
    links: ListLinks,
}

impl_link!(FileBlock, 0 => links);

pub struct InodeFile {
    blocks: List<FileBlock>,
}

impl InodeOperations for Mutex<InodeFile> {
    fn read(&self, mut offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let mut inner = self.lock();
        let mut cursor = inner.blocks.cursor_mut();
        let mut bytes_read = 0;

        while let Some(_) = cursor.get()
            && offset >= TMPFS_BLOCK_SIZE as u64
        {
            offset -= TMPFS_BLOCK_SIZE as u64;
            cursor.next();
        }
        while let Some(block) = cursor.get()
            && bytes_read < buffer.len()
        {
            let bytes_to_read_in_block = (buffer.len() - bytes_read)
                .min(block.contents.lock().size.saturating_sub(offset as usize));

            if bytes_to_read_in_block == 0 {
                break;
            }

            buffer[bytes_read..(bytes_read + bytes_to_read_in_block)].copy_from_slice(
                &block.contents.lock().block
                    [(offset as usize)..(offset as usize + bytes_to_read_in_block)],
            );

            bytes_read += bytes_to_read_in_block;

            offset -= offset;

            let _ = cursor.next();
        }

        Ok(bytes_read)
    }

    fn write(&self, mut offset: u64, buffer: &[u8]) -> FsResult<()> {
        let mut inner = self.lock();
        let mut cursor = inner.blocks.cursor_mut();
        let mut bytes_written = 0;

        while let Some(_) = cursor.get()
            && offset >= TMPFS_BLOCK_SIZE as u64
        {
            offset -= TMPFS_BLOCK_SIZE as u64;
            cursor.next();
        }
        while bytes_written < buffer.len() {
            if let Some(block) = cursor.get() {
                let mut contents = block.contents.lock();
                let bytes_to_write_in_block = (buffer.len() - bytes_written)
                    .min(TMPFS_BLOCK_SIZE.saturating_sub(offset as usize));
                if bytes_to_write_in_block == 0 {
                    break;
                }
                contents.block[(offset as usize)..(offset as usize + bytes_to_write_in_block)]
                    .copy_from_slice(
                        &buffer[bytes_written..(bytes_written + bytes_to_write_in_block)],
                    );

                contents.size = contents.size.max(offset as usize + bytes_to_write_in_block);

                bytes_written += bytes_to_write_in_block;

                offset -= offset;

                drop(contents);

                let _ = cursor.next();
            } else {
                cursor.insert_before(UniqueArc::<FileBlock>::zeroed().into());
                let _ = cursor.next_back();
            }
        }

        Ok(())
    }
}

pub struct InodeDirectory {
    children: Mutex<List<Inode>>,
}

impl InodeOperations for InodeDirectory {
    fn list_directory(&self, list: &mut List<ListLinkWrapper<Arc<Inode>>>) -> super::FsResult<()> {
        let mut children = self.children.lock();
        let mut cursor = children.cursor_mut();

        while let Some(inode) = cursor.get_arc() {
            let listarc: ListLinkWrapper<Arc<Inode>, 0> = ListLinkWrapper::new(inode.clone());
            let wrapped: ListArc<_, 0> = UniqueArc::new(listarc).into();
            list.push_back(wrapped);

            let _ = cursor.next();
        }
        Ok(())
    }

    fn create_file(&self, name: &str) -> FsResult<()> {
        let file = InodeFile {
            blocks: List::new(),
        };
        let node: ListArc<Inode, 0> = UniqueArc::new(Inode::new(Arc::new(Mutex::new(file)))).into();

        node.meta().set_name(name);

        self.children.lock().push_back(node);

        Ok(())
    }
}

impl InodeDirectory {
    fn remove_file(&self, name: &str) {
        let mut children = self.children.lock();
        let mut cursor = children.cursor_mut();

        while let Some(file) = cursor.get_arc() {
            if file.meta().name() == name {
                cursor.remove();
            } else {
                let _ = cursor.next();
            }
        }
    }

    fn create_directory(&self, name: &str) {
        let directory = InodeDirectory {
            children: Mutex::new(List::new()),
        };
        let node: ListArc<Inode, 0> = UniqueArc::new(Inode::new(Arc::new(directory))).into();

        node.meta().set_name(name);

        self.children.lock().push_back(node);
    }

    fn remove_directory(&self, name: &str) {
        self.remove_file(name);
    }
}

/// In-memory file system.
pub struct TmpFs {
    root: Arc<Inode>,
}

impl TmpFs {
    pub fn new() -> Self {
        let root = Arc::new(InodeDirectory {
            children: Mutex::new(List::new()),
        });
        let root = Inode::new(root);
        root.meta().set_name("");

        Self {
            root: Arc::new(root),
        }
    }
}

impl FileSystem for TmpFs {
    fn root(&self) -> Arc<Inode> {
        self.root.clone()
    }

    fn open(&self, path: &str) -> FsResult<Arc<Inode>> {
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

    fn create(&self, path: &str) -> FsResult<Arc<Inode>> {
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
}
