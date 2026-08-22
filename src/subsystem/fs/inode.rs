use core::ffi::CStr;

use crate::{
    impl_link,
    sched::{Mutex, MutexGuard},
    utils::{Arc, ArcAny, List, ListLinkWrapper, ListLinks, SpinLock, SpinLockGuard},
};

const PERMISSION_READ: u8 = 4;
const PERMISSION_WRITE: u8 = 2;
const PERMISSION_EXECUTE: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    Unsupported,
    NoExist,
}

pub type FsResult<T> = Result<T, FsError>;

pub trait InodeOperations: Send + Sync + 'static {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let _ = offset;
        let _ = buffer;

        Err(FsError::Unsupported)
    }

    fn write(&self, offset: u64, buffer: &[u8]) -> FsResult<()> {
        let _ = offset;
        let _ = buffer;

        Err(FsError::Unsupported)
    }

    fn list_directory(&self, list: &mut List<ListLinkWrapper<Arc<Inode>>>) -> FsResult<()> {
        let _ = list;

        Err(FsError::Unsupported)
    }

    fn create_file(&self, name: &str) -> FsResult<()> {
        let _ = name;

        Err(FsError::Unsupported)
    }

    fn create_directory(&self, name: &str) -> FsResult<()> {
        let _ = name;

        Err(FsError::Unsupported)
    }
}

pub struct InodeMeta {
    /// Name of the Inode
    pub name_buf: [u8; 32],
    /// Unique identifier for the Inode.
    pub uid: u64,
    /// RWX Permissions for the Inode
    pub permissions: u8,
    /// Number of bytes this inode represents
    pub file_size: u64,
    /// Misc data to be used by a driver.
    driver_data: Option<ArcAny>,
}

impl InodeMeta {
    pub fn name(&self) -> &str {
        CStr::from_bytes_until_nul(&self.name_buf)
            .unwrap()
            .to_str()
            .unwrap()
    }

    pub fn set_name(&mut self, name: &str) {
        self.name_buf[..name.len().min(32)].copy_from_slice(name.as_bytes());
    }
}

pub struct Inode {
    meta: SpinLock<InodeMeta>,
    operations: Mutex<Arc<dyn InodeOperations>>,
    pub links: ListLinks,
}

impl Inode {
    pub fn new(operations: Arc<dyn InodeOperations>) -> Self {
        Self {
            meta: SpinLock::new(InodeMeta {
                name_buf: [0u8; 32],
                uid: 0,
                permissions: PERMISSION_READ | PERMISSION_WRITE,
                file_size: 0,
                driver_data: None,
            }),
            operations: Mutex::new(operations),
            links: ListLinks::new(),
        }
    }

    pub fn meta(&self) -> SpinLockGuard<'_, InodeMeta> {
        self.meta.lock()
    }

    pub fn contents(&self) -> MutexGuard<'_, Arc<dyn InodeOperations>> {
        self.operations.lock()
    }
}

impl Inode {
    pub fn read(&self, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        self.contents().read(offset, buffer)
    }

    pub fn write(&self, offset: u64, buffer: &[u8]) -> FsResult<()> {
        self.contents().write(offset, buffer)
    }

    pub fn list_directory<R, F: FnOnce(&List<ListLinkWrapper<Arc<Inode>>>) -> R>(
        &self,
        func: F,
    ) -> FsResult<R> {
        let mut list = List::new();
        self.contents()
            .list_directory(&mut list)
            .map(|_| func(&list))
    }

    pub fn create_file(&self, name: &str) -> FsResult<()> {
        self.operations.lock().create_file(name)
    }

    pub fn create_directory(&self, name: &str) -> FsResult<()> {
        self.operations.lock().create_directory(name)
    }
}

impl_link!(Inode, 0 => links);
