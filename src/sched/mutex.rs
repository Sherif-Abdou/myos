use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering::SeqCst},
};

use crate::sched::WaitQueue;

pub struct Mutex<T> {
    inner: UnsafeCell<T>,
    available: AtomicBool,
    wait_queue: WaitQueue,
}

impl<T> Mutex<T> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            available: AtomicBool::new(false),
            wait_queue: WaitQueue::new(),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        loop {
            let should_block = self.wait_queue.prepare_enqueue(|| {
                !self
                    .available
                    .compare_exchange(false, true, SeqCst, SeqCst)
                    .is_ok()
            });
            if !should_block {
                return MutexGuard { mutex: self };
            } else {
                self.wait_queue.block();
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
        self.mutex.wait_queue.unblock_front();
    }
}
