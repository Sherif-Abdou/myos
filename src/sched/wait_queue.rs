use crate::{
    impl_link, sched::{SCHEDULER, Task, sched_yield}, utils::{Arc, List, ListLinks, SpinLock, UniqueArc},
};

pub struct WaitQueueNode {
    task: Arc<Task>,
    links: ListLinks,
}

impl_link!(WaitQueueNode, 0 => links);

pub struct WaitQueue {
    queue: SpinLock<Option<List<WaitQueueNode>>>,
}

impl WaitQueue {
    pub const fn new() -> Self {
        Self {
            queue: SpinLock::new(None),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().as_ref().is_none_or(|q| q.is_empty())
    }

    pub fn enqueue_and_block(&self) {
        self.enqueue();
        self.block();
    }

    pub fn block(&self) {
        SCHEDULER.get().unwrap().block_this_task();

        sched_yield();
    }

    pub fn enqueue(&self) {
        let this_task = SCHEDULER.get().unwrap().task();
        let mut wait_queue = self.queue.lock();
        if wait_queue.is_none() {
            *wait_queue = Some(List::new());
        }
        wait_queue.as_mut().unwrap().push_back(
            UniqueArc::new(WaitQueueNode {
                task: this_task.unwrap(),
                links: ListLinks::new(),
            })
            .into(),
        );
    }

    pub fn unblock_front(&self) {
        if let Some(waiter) = self
            .queue
            .lock()
            .as_mut()
            .and_then(|queue| queue.remove_front())
        {
            SCHEDULER.get().unwrap().unblock_task(&waiter.task);
        }
    }

    pub fn unblock_all(&self) {
        while let Some(waiter) = self
            .queue
            .lock()
            .as_mut()
            .and_then(|queue| queue.remove_front())
        {
            SCHEDULER.get().unwrap().unblock_task(&waiter.task);
        }
    }
}
