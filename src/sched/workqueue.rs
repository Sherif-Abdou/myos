use core::mem::transmute;

use crate::{
    impl_link,
    sched::{SCHEDULER, WaitQueue},
    utils::{Arc, ArcAny, List, ListLinks, SpinLock, UniqueArc},
};

struct WorkqueuEntry {
    function: fn(Option<ArcAny>),
    arg: Option<ArcAny>,
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
            let mut queue = workqueue.queue.lock();
            let is_empty = workqueue.waiter.prepare_enqueue(|| queue.is_empty());
            if is_empty {
                drop(queue);
                workqueue.waiter.block();
                continue;
            }
            let item = queue.remove_front();
            drop(queue);
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

    pub fn enqueue_work(&self, function: fn(Option<ArcAny>), arg: Option<ArcAny>) {
        self.queue.lock().push_back(
            UniqueArc::new(WorkqueuEntry {
                function,
                arg,
                links: ListLinks::new(),
            })
            .into(),
        );
        self.waiter.unblock_front();
    }
}
