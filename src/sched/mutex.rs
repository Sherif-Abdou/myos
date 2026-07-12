use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering::SeqCst},
};

use crate::{
    impl_link,
    sched::{SCHEDULER, Task},
    utils::{Arc, List, ListLinks, SpinLock, UniqueArc},
};

struct WaitQueueNode {
    task: Arc<Task>,
    links: ListLinks,
}

impl_link!(WaitQueueNode, 0 => links);

pub struct Mutex<T> {
    inner: UnsafeCell<T>,
    available: AtomicBool,
    wait_queue: SpinLock<List<WaitQueueNode>>,
}

impl<T> Mutex<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            available: AtomicBool::new(false),
            wait_queue: SpinLock::new(List::new()),
        }
    }

    pub fn lock(&self) {
        loop {
            if self
                .available
                .compare_exchange(false, true, SeqCst, SeqCst)
                .is_ok()
            {
                // Fast Path
            } else {
                let this_task = SCHEDULER.get().unwrap().task();

                self.wait_queue.lock().push_back(
                    UniqueArc::new(WaitQueueNode {
                        task: this_task.unwrap(),
                        links: ListLinks::new(),
                    })
                    .into(),
                );

                // TODO: SLEEP
            }
        }
    }
}
