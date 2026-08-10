mod tmpfs;

pub use tmpfs::TmpFs;

use core::ffi::CStr;

use crate::{
    impl_link,
    sched::{Mutex, MutexGuard},
    utils::{Arc, List, ListLinks, SpinLock, SpinLockGuard},
};

const PERMISSION_READ: u8 = 4;
const PERMISSION_WRITE: u8 = 2;
const PERMISSION_EXECUTE: u8 = 1;

pub trait InodeFileExt {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> usize;
    fn write(&self, offset: u64, buffer: &[u8]);
}

pub trait InodeDirectoryExt {
    fn list_directory(&self) -> MutexGuard<'_, List<Inode>>;

    fn create_file(&self, name: &str);
    fn remove_file(&self, name: &str);
    fn create_directory(&self, name: &str);
    fn remove_directory(&self, name: &str);
}

pub enum InodeContents {
    Directory(Arc<dyn InodeDirectoryExt>),
    File(Arc<dyn InodeFileExt>),
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
    contents: Mutex<InodeContents>,
    pub links: ListLinks,
}

impl Inode {
    pub fn new(contents: InodeContents) -> Self {
        Self {
            meta: SpinLock::new(InodeMeta {
                name_buf: [0u8; 32],
                uid: 0,
                permissions: PERMISSION_READ | PERMISSION_WRITE,
                file_size: 0,
            }),
            contents: Mutex::new(contents),
            links: ListLinks::new(),
        }
    }

    pub fn meta(&self) -> SpinLockGuard<'_, InodeMeta> {
        self.meta.lock()
    }

    pub fn contents(&self) -> MutexGuard<'_, InodeContents> {
        self.contents.lock()
    }
}

impl Inode {
    pub fn read(&self, offset: u64, buffer: &mut [u8]) -> usize {
        let contents = self.contents.lock();
        let InodeContents::File(ref contents) = *contents else {
            return 0;
        };

        contents.read(offset, buffer)
    }

    pub fn write(&self, offset: u64, buffer: &[u8]) {
        let contents = self.contents.lock();
        let InodeContents::File(ref contents) = *contents else {
            return;
        };

        contents.write(offset, buffer);
    }

    pub fn create_file(&self, name: &str) {
        let contents = self.contents.lock();
        let InodeContents::Directory(ref contents) = *contents else {
            return;
        };

        contents.create_file(name);
    }
    pub fn remove_file(&self, name: &str) {
        let contents = self.contents.lock();
        let InodeContents::Directory(ref contents) = *contents else {
            return;
        };

        contents.remove_file(name);
    }
    pub fn create_directory(&self, name: &str) {
        let contents = self.contents.lock();
        let InodeContents::Directory(ref contents) = *contents else {
            return;
        };

        contents.create_directory(name);
    }
    pub fn remove_directory(&self, name: &str) {
        let contents = self.contents.lock();
        let InodeContents::Directory(ref contents) = *contents else {
            return;
        };

        contents.remove_directory(name);
    }

    pub fn list_directory<R, F: FnOnce(MutexGuard<'_, List<Inode>>) -> R>(&self, func: F) -> R {
        let contents = self.contents.lock();
        let InodeContents::Directory(ref contents) = *contents else {
            panic!("list_directory can only be called on a directory.");
        };

        func(contents.list_directory())
    }
}

impl_link!(Inode, 0 => links);

pub trait FileSystem {
    fn root(&self) -> Arc<Inode>;

    fn create_file(&self, name: &str) {
        self.root().create_file(name);
    }

    fn remove_file(&self, name: &str) {
        self.root().remove_file(name);
    }

    fn create_directory(&self, name: &str) {
        self.root().create_directory(name);
    }

    fn remove_directory(&self, name: &str) {
        self.root().remove_directory(name);
    }

    fn list_directory<R, F: FnOnce(MutexGuard<'_, List<Inode>>) -> R>(&self, func: F) -> R {
        self.root().list_directory(func)
    }

    fn open(&self, path: &str) -> Option<Arc<Inode>> {
        let parts = path.split("/");

        let mut current = self.root();
        for part in parts {
            if part.is_empty() {
                continue;
            }

            let potential_child = current.list_directory(|mut list| {
                let mut cursor = list.cursor_mut();
                while cursor
                    .get()
                    .is_some_and(|child| child.meta().name() != part)
                {
                    let _ = cursor.next();
                }

                cursor.get_arc()
            });

            let child = potential_child?;
            current = child
        }
        Some(current)
    }

    fn create(&self, path: &str) -> Option<Arc<Inode>> {
        let parts = path.split("/");
        let num_parts = path.chars().filter(|c| *c == '/').count();

        let mut current = self.root();
        for part in parts.take(num_parts - 1) {
            if part.is_empty() {
                continue;
            }

            let potential_child = current.list_directory(|mut list| {
                let mut cursor = list.cursor_mut();
                while cursor
                    .get()
                    .is_some_and(|child| child.meta().name() != part)
                {
                    let _ = cursor.next();
                }

                cursor.get_arc()
            });

            let child = potential_child?;
            current = child
        }

        let child_to_create = path.split('/').nth(num_parts - 1)?;
        current.create_file(child_to_create);

        current.list_directory(|mut list| {
            let mut cursor = list.cursor_mut();
            while cursor
                .get()
                .is_some_and(|child| child.meta().name() != child_to_create)
            {
                let _ = cursor.next();
            }

            cursor.get_arc()
        })
    }
}
