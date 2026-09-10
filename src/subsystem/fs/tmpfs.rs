use crate::{
    impl_link,
    sched::Mutex,
    subsystem::{
        FileSystem, FsResult, InodeDirectoryEntry,
        fs::{Inode, InodeOperations},
    },
    utils::{Arc, List, ListArc, ListLinks, SpinLock, UniqueArc},
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

    fn write(&self, mut offset: u64, buffer: &[u8]) -> FsResult<usize> {
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
                let _ = cursor.back();
            }
        }

        Ok(bytes_written)
    }
}

pub struct InodeDirectory {
    children: Mutex<List<InodeDirectoryEntry>>,
}

impl InodeOperations for InodeDirectory {
    fn list_directory(&self, list: &mut List<InodeDirectoryEntry>) -> super::FsResult<()> {
        let mut children = self.children.lock();
        let mut cursor = children.cursor_mut();

        while let Some(inode) = cursor.get_arc() {
            let listarc = (*inode).clone();
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
        let inode = Arc::new(Inode::new(Arc::new(Mutex::new(file))));

        let node: ListArc<InodeDirectoryEntry, 0> =
            UniqueArc::new(InodeDirectoryEntry::new(name, inode)).into();

        self.children.lock().push_back(node);

        Ok(())
    }

    fn create_file_with_ops(&self, name: &str, ops: Arc<dyn InodeOperations>) -> FsResult<()> {
        let inode = Arc::new(Inode::new(ops));
        let node: ListArc<InodeDirectoryEntry, 0> =
            UniqueArc::new(InodeDirectoryEntry::new(name, inode)).into();

        self.children.lock().push_back(node);

        Ok(())
    }
}

impl InodeDirectory {
    fn remove_file(&self, name: &str) {
        let mut children = self.children.lock();
        let mut cursor = children.cursor_mut();

        while let Some(file) = cursor.get_arc() {
            if file.name() == name {
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
        let inode = Arc::new(Inode::new(Arc::new(directory)));
        let node: ListArc<InodeDirectoryEntry, 0> =
            UniqueArc::new(InodeDirectoryEntry::new(name, inode)).into();

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
                while cursor.get().is_some_and(|child| child.name() != part) {
                    let _ = cursor.next();
                }

                cursor.get_arc()
            })?;

            let child = potential_child.ok_or(FsError::NoExist)?;
            current = child.inode().clone();
        }
        Ok(current)
    }

    fn create(&self, path: &str) -> FsResult<Arc<Inode>> {
        let parts = path.split("/");
        let num_parts = path.chars().filter(|c| *c == '/').count();

        let mut current = self.root();
        for part in parts.take(num_parts.saturating_sub(1)) {
            if part.is_empty() {
                continue;
            }

            let potential_child = current.list_directory(|list| {
                let mut cursor = list.cursor();
                while cursor.get().is_some_and(|child| child.name() != part) {
                    let _ = cursor.next();
                }

                cursor.get_arc()
            })?;

            let child = potential_child.ok_or(FsError::NoExist)?;
            current = child.inode().clone();
        }

        let child_to_create = path.split('/').nth(num_parts).unwrap();
        current.create_file(child_to_create)?;

        current.list_directory(|list| {
            let mut cursor = list.cursor();
            while cursor
                .get()
                .is_some_and(|child| child.name() != child_to_create)
            {
                let _ = cursor.next();
            }

            cursor
                .get_arc()
                .map(|wrapper| wrapper.inode().clone())
                .ok_or(FsError::NoExist)
        })?
    }

    fn create_with_ops(&self, path: &str, ops: Arc<dyn InodeOperations>) -> FsResult<Arc<Inode>> {
        let parts = path.split("/");
        let num_parts = path.chars().filter(|c| *c == '/').count();

        let mut current = self.root();
        for part in parts.take(num_parts.saturating_sub(1)) {
            if part.is_empty() {
                continue;
            }

            let potential_child = current.list_directory(|list| {
                let mut cursor = list.cursor();
                while cursor.get().is_some_and(|child| child.name() != part) {
                    let _ = cursor.next();
                }

                cursor.get_arc()
            })?;

            let child = potential_child.ok_or(FsError::NoExist)?;
            current = child.inode().clone();
        }

        let child_to_create = path.split('/').nth(num_parts).unwrap();
        current.create_file_with_ops(child_to_create, ops)?;

        current.list_directory(|list| {
            let mut cursor = list.cursor();
            while cursor
                .get()
                .is_some_and(|child| child.name() != child_to_create)
            {
                let _ = cursor.next();
            }

            cursor
                .get_arc()
                .map(|wrapper| wrapper.inode().clone())
                .ok_or(FsError::NoExist)
        })?
    }
}
