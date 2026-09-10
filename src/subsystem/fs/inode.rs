use crate::{
    impl_link,
    sched::{Mutex, MutexGuard},
    utils::{Arc, ArcAny, KString, List, ListLinks, SpinLock, SpinLockGuard},
};

const PERMISSION_READ: u8 = 4;
const PERMISSION_WRITE: u8 = 2;
const PERMISSION_EXECUTE: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    /// Operation is not supported.
    Unsupported,
    /// Desired target does not exist.
    NoExist,
    /// No space for the desired resource on the device.
    NoSpace,
    /// Bad metadata for the device being read.
    BadMeta,
    /// Nothing more to read.
    EndOfFile,
}

pub type FsResult<T> = Result<T, FsError>;

pub trait InodeOperations: Send + Sync + 'static {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let _ = offset;
        let _ = buffer;

        Err(FsError::Unsupported)
    }

    fn write(&self, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let _ = offset;
        let _ = buffer;

        Err(FsError::Unsupported)
    }

    fn list_directory(&self, list: &mut List<InodeDirectoryEntry>) -> FsResult<()> {
        let _ = list;

        Err(FsError::Unsupported)
    }

    fn create_file(&self, name: &str) -> FsResult<()> {
        let _ = name;

        Err(FsError::Unsupported)
    }

    fn create_file_with_ops(&self, name: &str, ops: Arc<dyn InodeOperations>) -> FsResult<()> {
        let _ = name;
        let _ = ops;

        Err(FsError::Unsupported)
    }

    fn create_directory(&self, name: &str) -> FsResult<()> {
        let _ = name;

        Err(FsError::Unsupported)
    }
}

pub struct InodeMeta {
    /// Unique identifier for the Inode.
    pub uid: u64,
    /// RWX Permissions for the Inode
    pub permissions: u8,
    /// Number of bytes this inode represents
    pub file_size: u64,
    /// Misc data to be used by a driver.
    driver_data: Option<ArcAny>,
}

pub struct Inode {
    meta: SpinLock<InodeMeta>,
    operations: Mutex<Arc<dyn InodeOperations>>,
    pub links: ListLinks,
}

pub struct InodeDirectoryEntry {
    name: KString,
    inode: Arc<Inode>,
    links: ListLinks,
}

impl Clone for InodeDirectoryEntry {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            inode: self.inode.clone(),
            links: ListLinks::new(),
        }
    }
}

impl_link!(InodeDirectoryEntry, 0 => links);

impl InodeDirectoryEntry {
    pub fn new(name: impl AsRef<str>, inode: Arc<Inode>) -> Self {
        Self {
            name: KString::from_str(name.as_ref()),
            inode,
            links: ListLinks::new(),
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn inode(&self) -> &Arc<Inode> {
        &self.inode
    }
}

impl Inode {
    pub fn new(operations: Arc<dyn InodeOperations>) -> Self {
        Self {
            meta: SpinLock::new(InodeMeta {
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

    pub fn write(&self, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        self.contents().write(offset, buffer)
    }

    pub fn list_directory<R, F: FnOnce(&List<InodeDirectoryEntry>) -> R>(
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

    pub fn create_file_with_ops(&self, name: &str, ops: Arc<dyn InodeOperations>) -> FsResult<()> {
        self.operations.lock().create_file_with_ops(name, ops)
    }

    pub fn create_directory(&self, name: &str) -> FsResult<()> {
        self.operations.lock().create_directory(name)
    }
}

impl_link!(Inode, 0 => links);
