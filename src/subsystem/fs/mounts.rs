use crate::{
    allocators::{KBox, KERNEL_ALLOCATOR},
    impl_link,
    sched::{Mutex, SCHEDULER},
    subsystem::FileSystem,
    utils::{Arc, KString, List, ListLinks, OnceSpinLock, UniqueArc},
};

use super::Inode;

pub struct MountTableEntry {
    fs: KBox<dyn FileSystem + Send + Sync + 'static>,
    path: KBox<str>,
    links: ListLinks,
}

impl MountTableEntry {
    pub fn fs(&self) -> &dyn FileSystem {
        &*self.fs
    }
}

impl_link!(MountTableEntry, 0 => links);

pub struct MountTable {
    mounts: Mutex<List<MountTableEntry>>,
    root: Mutex<Arc<Inode>>,
}

pub static MOUNT_TABLE: OnceSpinLock<MountTable> = OnceSpinLock::new();

impl MountTable {
    pub fn new(root_fs: KBox<dyn FileSystem + Send + Sync + 'static>) -> Self {
        let root_inode = root_fs.root();
        let mut list = List::new();
        list.push_back(
            UniqueArc::new(MountTableEntry {
                fs: root_fs,
                path: KBox::clone_from_ref_in("/", &KERNEL_ALLOCATOR),
                links: ListLinks::new(),
            })
            .into(),
        );

        Self {
            mounts: Mutex::new(list),
            root: Mutex::new(root_inode),
        }
    }

    pub fn lookup_mount(&self, path: &str) -> Option<Arc<MountTableEntry>> {
        let mounts = self.mounts.lock();
        let mut cursor = mounts.cursor();
        while let Some(mount) = cursor.get_arc() {
            if mount.path.starts_with(path) {
                return Some(mount);
            }
            let _ = cursor.next();
        }
        None
    }

    pub fn mount(&self, path: &str, fs: KBox<dyn FileSystem + Send + Sync + 'static>) {
        assert!(self.lookup_mount(path).is_none());
        let mut list = self.mounts.lock();

        list.push_front(
            UniqueArc::new(MountTableEntry {
                fs,
                path: KBox::clone_from_ref_in(path, &KERNEL_ALLOCATOR),
                links: ListLinks::new(),
            })
            .into(),
        );
    }
}

impl FileSystem for MountTable {
    fn root(&self) -> crate::utils::Arc<Inode> {
        self.root.lock().clone()
    }

    fn create(&self, path: &str, flags: super::FileType) -> super::FsResult<Arc<Inode>> {
        let wkdir = SCHEDULER.get().unwrap().local_task().map(|task| {
            let mut task = task.with_wkdir(KString::from_str);
            task.extend_from_string(path);

            task
        });
        let path = if !path.starts_with("/") {
            wkdir.as_ref().map(|wkdir| wkdir.as_str()).unwrap_or("")
        } else {
            path
        };
        for mount in self.mounts.lock().cursor() {
            if path.starts_with(&*mount.path) {
                let effective_path = path.strip_prefix(&*mount.path).unwrap();

                return mount.fs.create(effective_path, flags);
            }
        }

        Err(super::FsError::NoExist)
    }

    fn create_with_ops(
        &self,
        path: &str,
        ops: Arc<dyn super::InodeOperations>,
    ) -> super::FsResult<Arc<Inode>> {
        let wkdir = SCHEDULER.get().unwrap().local_task().map(|task| {
            let mut task = task.with_wkdir(KString::from_str);
            task.extend_from_string(path);

            task
        });
        let path = if !path.starts_with("/") {
            wkdir.as_ref().map(|wkdir| wkdir.as_str()).unwrap_or("")
        } else {
            path
        };
        for mount in self.mounts.lock().cursor() {
            if path.starts_with(&*mount.path) {
                let effective_path = path.strip_prefix(&*mount.path).unwrap();

                return mount.fs.create_with_ops(effective_path, ops);
            }
        }

        Err(super::FsError::NoExist)
    }

    fn open(&self, path: &str) -> super::FsResult<Arc<Inode>> {
        let wkdir = SCHEDULER.get().unwrap().local_task().map(|task| {
            let mut task = task.with_wkdir(KString::from_str);
            task.extend_from_string(path);

            task
        });
        let path = if !path.starts_with("/") {
            wkdir.as_ref().map(|wkdir| wkdir.as_str()).unwrap_or("")
        } else {
            path
        };
        for mount in self.mounts.lock().cursor() {
            if path.starts_with(&*mount.path) {
                let effective_path = path.strip_prefix(&*mount.path).unwrap();

                return mount.fs.open(effective_path);
            }
        }

        Err(super::FsError::NoExist)
    }

    fn remove(&self, path: &str, flags: super::FileType) -> super::FsResult<()> {
        let wkdir = SCHEDULER.get().unwrap().local_task().map(|task| {
            let mut task = task.with_wkdir(KString::from_str);
            task.extend_from_string(path);

            task
        });
        let path = if !path.starts_with("/") {
            wkdir.as_ref().map(|wkdir| wkdir.as_str()).unwrap_or("")
        } else {
            path
        };
        for mount in self.mounts.lock().cursor() {
            if path.starts_with(&*mount.path) {
                let effective_path = path.strip_prefix(&*mount.path).unwrap();

                return mount.fs.remove(effective_path, flags);
            }
        }

        Err(super::FsError::NoExist)
    }
}
