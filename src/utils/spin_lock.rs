use core::{
    arch::asm,
    cell::UnsafeCell,
    hint::spin_loop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
};

use crate::printk;

/// Primitive Spinlock, provides synchronization between multiple cores.
pub struct SpinLock<T> {
    inner: UnsafeCell<T>,
    lock: AtomicUsize,
}

impl<T> SpinLock<T> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            lock: AtomicUsize::new(0),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        let mut daif: usize = 0;

        unsafe {
            asm!("mrs {x}, daif", x = out(reg) daif);
            asm!("msr daifset, #0b0010");
        };
        // Spin until we get the lock.
        while self
            .lock
            .compare_exchange(0x0, 0x1, SeqCst, SeqCst)
            .is_err()
        {
            spin_loop();
        }

        SpinLockGuard { lock: self, daif }
    }
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
    daif: usize,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.inner.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.inner.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        let daif = self.daif;
        // Free the lock.
        self.lock
            .lock
            .store(0, core::sync::atomic::Ordering::SeqCst);
        unsafe {
            asm!("msr daif, {x}", x = in(reg) daif);
        };
    }
}

pub struct OnceSpinLock<T> {
    inner: UnsafeCell<Option<T>>,
    initialized: AtomicBool,
    lock: SpinLock<()>,
}

unsafe impl<T: Sync> Sync for OnceSpinLock<T> {}

impl<T> OnceSpinLock<T> {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(None),
            initialized: AtomicBool::new(false),
            lock: SpinLock::new(()),
        }
    }

    pub fn get(&self) -> Option<&T> {
        if self.initialized.load(SeqCst) {
            unsafe { self.inner.get().as_ref().unwrap().as_ref() }
        } else {
            None
        }
    }

    pub fn set(&self, value: T) -> Result<(), T> {
        // Hold the lock during this function.
        let _lock = self.lock.lock();

        if !self.initialized.load(SeqCst) {
            unsafe { self.inner.get().write(Some(value)) };
            self.initialized.store(true, SeqCst);
            Ok(())
        } else {
            Err(value)
        }
    }
}
