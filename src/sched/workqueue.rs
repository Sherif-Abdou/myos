use core::{any::Any, arch::asm, mem::transmute};

use crate::{
    early_printk, impl_link, printk,
    sched::{SCHEDULER, WaitQueue},
    utils::{Arc, List, ListLinks, SpinLock, UniqueArc},
};

struct WorkqueuEntry {
    function: fn(Option<Arc<dyn Any + Send + Sync + 'static>>),
    arg: Option<Arc<dyn Any + Send + Sync + 'static>>,
    links: ListLinks,
}

impl_link!(WorkqueuEntry, 0 => links);

pub struct Workqueue {
    queue: SpinLock<List<WorkqueuEntry>>,
    waiter: WaitQueue,
}

impl Workqueue {
    fn wq_worker(arg: *mut ()) {
        let workqueue: Arc<Workqueue> = unsafe { transmute(arg) };

        loop {
            let queue = workqueue.queue.lock();
            if queue.is_empty() {
                workqueue.waiter.enqueue_and_block();
                drop(queue);
                continue;
            }
            drop(queue);
            let item = workqueue.queue.lock().remove_front();
            if let Some(item) = item {
                (item.function)(item.arg.clone());
            }
        }
    }

    pub fn create_workqueue() -> Arc<Self> {
        let queue = Arc::new(Workqueue {
            queue: SpinLock::new(List::new()),
            waiter: WaitQueue::new(),
        });

        SCHEDULER
            .get()
            .unwrap()
            .task_from_fn(Self::wq_worker, unsafe {
                transmute::<Arc<Workqueue>, *mut ()>(queue.clone())
            });

        queue
    }

    pub fn enqueue_work(
        &self,
        function: fn(Option<Arc<dyn Any + Send + Sync + 'static>>),
        arg: Option<Arc<dyn Any + Send + Sync + 'static>>,
    ) {
        self.queue.lock().push_back(
            UniqueArc::new(WorkqueuEntry {
                function,
                arg,
                links: ListLinks::new(),
            })
            .into(),
        );
        // self.waiter.unblock_front();
    }
}
