use core::{
    arch::asm,
    cell::UnsafeCell,
    mem::{MaybeUninit, zeroed},
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
};

pub struct CoreLock<T> {
    inner: UnsafeCell<T>,
    state: AtomicU64,
}

unsafe impl<T> Sync for CoreLock<T> {}

fn with_core_critical_section<R, F: FnOnce() -> R>(f: F) -> R {
    let mut daif: u64 = 0;

    unsafe {
        asm!("mrs {x}, daif", x = out(reg) daif);
    };
    let r = f();
    unsafe {
        asm!("msr daif, {x}", x = in(reg) daif);
    };

    r
}

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

pub struct LazyCoreLock<T, F = fn() -> T> {
    inner: UnsafeCell<MaybeUninit<T>>,
    f: F,
    initialized: AtomicBool,
}

unsafe impl<T: Sync, F> Sync for LazyCoreLock<T, F> {}

impl<T, F: Fn() -> T> LazyCoreLock<T, F> {
    pub const fn new(f: F) -> Self {
        Self {
            inner: UnsafeCell::new(MaybeUninit::uninit()),
            f,
            initialized: AtomicBool::new(false),
        }
    }

    pub fn get<'a>(&'a self) -> &'a T {
        if !self.initialized.load(Relaxed) {
            unsafe { self.inner.get().write(MaybeUninit::new((self.f)())) }
            self.initialized.store(true, Relaxed);
        }
        unsafe { self.inner.get().as_ref().unwrap().assume_init_ref() }
    }
}
