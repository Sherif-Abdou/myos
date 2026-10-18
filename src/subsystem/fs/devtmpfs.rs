use crate::{
    allocators::{KVecDeque, kvecdeque},
    sched::Workqueue,
    subsystem::{FileSystem, InodeOperations, TmpFs},
    utils::{Arc, ArcAny, KString, OnceSpinLock, SpinLock, expect_downcast_ref},
};

pub enum DevTmpFsOp {
    AddDevice {
        name: KString,
        ops: Arc<dyn InodeOperations>,
    },
    RemoveDevice {
        name: KString,
    },
}

pub struct DevTmpFs {
    tmpfs: Arc<TmpFs>,
    operation_queue: SpinLock<KVecDeque<DevTmpFsOp>>,
    wq: Arc<Workqueue>,
}

fn service_devtmpfs_operation(arg: Option<ArcAny>) {
    let devtmpfs: &DevTmpFs = expect_downcast_ref(arg.as_ref());
    let operation = devtmpfs.operation_queue.lock().pop_front();

    let Some(operation) = operation else {
        return;
    };

    match operation {
        DevTmpFsOp::AddDevice { name, ops } => {
            let _ = devtmpfs.tmpfs.create_with_ops(name.as_str(), ops);
        }
        DevTmpFsOp::RemoveDevice { name } => {
            let _ = devtmpfs.tmpfs.remove(name.as_str(), super::FileType::File);
        }
    }
}

static DEVTMPFS: OnceSpinLock<Arc<DevTmpFs>> = OnceSpinLock::new();

pub fn create_devtmpfs() {
    let _ = DEVTMPFS.set(DevTmpFs::create_devtmpfs());
}

pub fn devtmpfs() -> &'static Arc<DevTmpFs> {
    DEVTMPFS.get().unwrap()
}

impl DevTmpFs {
    fn new() -> Self {
        Self {
            tmpfs: Arc::new(TmpFs::new()),
            operation_queue: SpinLock::new(kvecdeque()),
            wq: Workqueue::create_workqueue(),
        }
    }

    pub fn create_devtmpfs() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl Arc<DevTmpFs> {
    pub fn push_operation(&self, op: DevTmpFsOp) {
        self.operation_queue.lock().push_back(op);
        self.wq
            .enqueue_work(service_devtmpfs_operation, Some(self.clone()));
    }
}

impl FileSystem for Arc<DevTmpFs> {
    fn root(&self) -> crate::utils::Arc<super::Inode> {
        self.tmpfs.root()
    }

    fn open(&self, path: &str) -> super::FsResult<crate::utils::Arc<super::Inode>> {
        self.tmpfs.open(path)
    }

    fn create(
        &self,
        path: &str,
        flags: crate::subsystem::FileType,
    ) -> super::FsResult<crate::utils::Arc<super::Inode>> {
        self.tmpfs.create(path, flags)
    }

    fn create_with_ops(
        &self,
        path: &str,
        ops: crate::utils::Arc<dyn super::InodeOperations>,
    ) -> super::FsResult<crate::utils::Arc<super::Inode>> {
        self.tmpfs.create_with_ops(path, ops)
    }

    fn remove(&self, path: &str, flags: super::FileType) -> super::FsResult<()> {
        self.tmpfs.remove(path, flags)
    }
}
