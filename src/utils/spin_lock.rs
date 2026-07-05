use core::{
    arch::asm,
    cell::UnsafeCell,
    hint::spin_loop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

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
            .compare_exchange(0x0, daif, SeqCst, SeqCst)
            .is_err()
        {
            spin_loop();
        }

        SpinLockGuard { lock: self }
    }
}

unsafe impl<T> Sync for SpinLock<T> {}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
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
        let daif = self.lock.lock.load(core::sync::atomic::Ordering::SeqCst);

        unsafe {
            asm!("msr daif, {x}", x = in(reg) daif);
        };

        // Free the lock.
        self.lock
            .lock
            .store(0, core::sync::atomic::Ordering::SeqCst);
    }
}
