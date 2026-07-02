use core::{
    arch::asm,
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::atomic::AtomicU64,
};

pub struct CoreLock<T> {
    inner: UnsafeCell<T>,
    state: AtomicU64,
}

unsafe impl<T> Sync for CoreLock<T> {}

impl<T> CoreLock<T> {
    pub const fn new(inner: T) -> Self {
        Self {
            inner: UnsafeCell::new(inner),
            state: AtomicU64::new(0),
        }
    }

    pub fn lock(&self) -> CoreLockGuard<'_, T> {
        self.save_daif();

        CoreLockGuard {
            inner: unsafe { NonNull::new_unchecked(self.inner.get()) },
            lock: self,
        }
    }

    fn save_daif(&self) {
        let mut x: u64 = 0;
        unsafe {
            asm!("mrs {x}, daif", x = out(reg) x);
        };
        self.state.store(x, core::sync::atomic::Ordering::Relaxed);
    }

    fn restore_daif(&self) {
        let x = self.state.load(core::sync::atomic::Ordering::Relaxed);

        unsafe {
            asm!("msr daif, {x}", x = in(reg) x);
        };
    }
}

pub struct CoreLockGuard<'a, T> {
    inner: NonNull<T>,
    lock: &'a CoreLock<T>,
}

impl<T> Deref for CoreLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.inner.as_ref() }
    }
}

impl<T> DerefMut for CoreLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.inner.as_mut() }
    }
}

impl<T> Drop for CoreLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.restore_daif();
    }
}
