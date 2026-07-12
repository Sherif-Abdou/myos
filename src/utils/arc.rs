use core::{
    alloc::Layout,
    mem::{ManuallyDrop, offset_of},
    ops::{Deref, DerefMut},
    ptr::{NonNull, drop_in_place},
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

use alloc::alloc::Allocator;

use crate::{allocators::KERNEL_ALLOCATOR, utils::ListLinks};

pub struct ArcInner<T: ?Sized> {
    count: AtomicUsize,
    inner: T,
}

impl<T: ?Sized> ArcInner<T> {
    pub fn as_raw(&self) -> *const T {
        &raw const self.inner
    }
}

pub fn arc_inner_offset<T>() -> usize {
    offset_of!(ArcInner<T>, inner)
}

pub struct Arc<T> {
    inner: NonNull<ArcInner<T>>,
}

impl<T> Clone for Arc<T> {
    fn clone(&self) -> Self {
        unsafe {
            self.inner.as_ref().count.fetch_add(1, SeqCst);
        }
        Self { inner: self.inner }
    }
}

impl<T> Arc<T> {
    pub fn new(value: T) -> Self {
        let memory = KERNEL_ALLOCATOR
            .allocate(Layout::new::<ArcInner<T>>())
            .unwrap();
        let mut inner: NonNull<ArcInner<T>> = memory.cast();

        unsafe { inner.as_mut().count = AtomicUsize::new(1) };
        unsafe { (&raw mut inner.as_mut().inner).write(value) };

        Self { inner }
    }

    pub fn make_unique(self: Arc<T>) -> Result<UniqueArc<T>, Arc<T>> {
        if unsafe {
            self.inner
                .as_ref()
                .count
                .compare_exchange(1, 1, SeqCst, SeqCst)
                .is_ok()
        } {
            Ok(UniqueArc { inner: self })
        } else {
            Err(self)
        }
    }

    pub unsafe fn as_inner_ptr(&self) -> *const ArcInner<T> {
        self.inner.as_ptr()
    }

    pub unsafe fn as_ptr(&self) -> *const T {
        self.deref()
    }
}

impl<T> Deref for Arc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &self.inner.as_ref().inner }
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        let inner = unsafe { self.inner.as_ref() };
        if inner.count.fetch_sub(1, SeqCst) == 1 {
            unsafe { drop_in_place(self.inner.as_ptr()) };
            unsafe { KERNEL_ALLOCATOR.deallocate(self.inner.cast(), Layout::new::<ArcInner<T>>()) };
        }
    }
}

pub struct UniqueArc<T> {
    inner: Arc<T>,
}

impl<T> UniqueArc<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub unsafe fn as_ptr(&self) -> *const T {
        unsafe { self.inner.as_ptr() }
    }

    pub unsafe fn as_mut(&self) -> *mut T {
        unsafe { self.inner.as_ptr() as _ }
    }
}

impl<T> Deref for UniqueArc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for UniqueArc<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut self.inner.inner.as_mut().inner }
    }
}

pub struct ListArc<T, const N: usize> {
    inner: Arc<T>,
}

impl<T, const N: usize> Deref for ListArc<T, N> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, const N: usize> From<UniqueArc<T>> for ListArc<T, N> {
    fn from(value: UniqueArc<T>) -> Self {
        ListArc { inner: value.inner }
    }
}

impl<T, const N: usize> ListArc<T, N> {
    pub unsafe fn into_arc_inner(self) -> NonNull<ArcInner<T>> {
        let arc = ManuallyDrop::new(self);

        arc.inner.inner
    }

    pub unsafe fn from_arc_inner(inner: NonNull<ArcInner<T>>) -> Self {
        Self {
            inner: Arc { inner },
        }
    }

    pub fn clone_arc(&self) -> Arc<T> {
        self.inner.clone()
    }
}

pub trait LinkedNode<T, const N: usize> {
    fn arc_from_link(link: *mut ListLinks) -> *mut ArcInner<T>;

    fn link_from_arc(arc: *mut ArcInner<T>) -> *mut ListLinks;
}
