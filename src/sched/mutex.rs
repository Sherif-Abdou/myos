use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering::SeqCst},
};

use crate::{
    early_printk, impl_link, sched::{SCHEDULER, Task, sched_yield}, utils::{Arc, List, ListLinks, SpinLock, UniqueArc},
};

struct WaitQueueNode {
    task: Arc<Task>,
    links: ListLinks,
}

impl_link!(WaitQueueNode, 0 => links);

pub struct Mutex<T> {
    inner: UnsafeCell<T>,
    available: AtomicBool,
    wait_queue: SpinLock<Option<List<WaitQueueNode>>>,
}

impl<T> Mutex<T> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            available: AtomicBool::new(false),
            wait_queue: SpinLock::new(None),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        loop {
            if self
                .available
                .compare_exchange(false, true, SeqCst, SeqCst)
                .is_ok()
            {
                return MutexGuard { mutex: self };
            } else {
                let this_task = SCHEDULER.get().unwrap().task();
                let mut wait_queue = self.wait_queue.lock();

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

                SCHEDULER.get().unwrap().block_this_task();

                sched_yield();
            }
        }
    }
}

unsafe impl<T> Sync for Mutex<T> {}

pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.mutex.inner.get().as_ref().unwrap() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.mutex.inner.get().as_mut().unwrap() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.available.store(false, SeqCst);
        if let Some(waiter) = self
            .mutex
            .wait_queue
            .lock()
            .as_mut()
            .and_then(|queue| queue.remove_front())
        {
            SCHEDULER.get().unwrap().unblock_task(&waiter.task);
        }
    }
}
