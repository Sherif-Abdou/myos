use crate::{
    impl_link,
    sched::{Mutex, MutexGuard},
    subsystem::fs::{Inode, InodeContents, InodeDirectoryExt, InodeFileExt},
    utils::{Arc, List, ListArc, ListLinks, SpinLock, UniqueArc},
};

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

impl InodeFileExt for Mutex<InodeFile> {
    fn read(&self, mut offset: u64, buffer: &mut [u8]) -> usize {
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

        bytes_read
    }

    fn write(&self, mut offset: u64, buffer: &[u8]) {
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
    }
}

pub struct InodeDirectory {
    children: Mutex<List<Inode>>,
}

impl InodeDirectoryExt for InodeDirectory {
    fn children(&self) -> MutexGuard<'_, List<Inode>> {
        self.children.lock()
    }
}

pub struct TmpFs {
    root: Inode,
}

impl TmpFs {
    pub fn new() -> Self {
        let root = Arc::new(InodeDirectory {
            children: Mutex::new(List::new()),
        });
        let root = Inode::new(InodeContents::Children(root));
        root.meta().set_name("");

        Self { root }
    }

    pub fn create_file(&self, name: &str) -> Arc<Inode> {
        let InodeContents::Children(ref directory) = *self.root.contents() else {
            panic!("Why is root not a directory?");
        };
        let mut children = directory.children();
        let file = InodeFile {
            blocks: List::new(),
        };
        let node: ListArc<Inode, 0> = UniqueArc::new(Inode::new(InodeContents::Content(Arc::new(
            Mutex::new(file),
        ))))
        .into();

        let ret = node.clone_arc();

        node.meta().set_name(name);

        children.push_back(node);

        ret
    }
}
