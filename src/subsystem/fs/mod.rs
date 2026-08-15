mod ext2;
mod inode;
mod tmpfs;

use crate::utils::{Arc, List, ListLinkWrapper};

pub use ext2::Ext2Fs;
pub use inode::*;
pub use tmpfs::TmpFs;

#[allow(unused_variables)]
pub trait FileSystem {
    fn root(&self) -> Arc<Inode>;

    fn create_file(&self, path: &str) -> FsResult<()> {
        Err(FsError::Unsupported)
    }

    fn remove_file(&self, path: &str) -> FsResult<()> {
        Err(FsError::Unsupported)
    }

    fn create_directory(&self, path: &str) -> FsResult<()> {
        Err(FsError::Unsupported)
    }

    fn remove_directory(&self, path: &str) -> FsResult<()> {
        Err(FsError::Unsupported)
    }

    fn list_directory<R, F: FnOnce(&List<ListLinkWrapper<Arc<Inode>>>) -> R>(
        &self,
        func: F,
    ) -> FsResult<R> {
        self.root().list_directory(func)
    }

    fn open(&self, path: &str) -> FsResult<Arc<Inode>> {
        Err(FsError::Unsupported)
    }

    fn create(&self, path: &str) -> FsResult<Arc<Inode>> {
        Err(FsError::Unsupported)
    }
}
