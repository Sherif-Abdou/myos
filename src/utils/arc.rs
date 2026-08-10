use core::{
    alloc::Layout,
    marker::Unsize,
    mem::{ManuallyDrop, offset_of},
    ops::{CoerceUnsized, Deref, DerefMut},
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

    pub fn make_arc(&self) -> Arc<T> {
        self.count.fetch_add(1, SeqCst);
        let inner = (&raw const *self) as *mut ArcInner<T>;
        Arc {
            inner: unsafe { NonNull::new_unchecked(inner) },
        }
    }
}

impl<T> Arc<T> {
    pub fn zeroed() -> Self {
        let memory = KERNEL_ALLOCATOR
            .allocate_zeroed(Layout::new::<ArcInner<T>>())
            .unwrap();
        let mut inner: NonNull<ArcInner<T>> = memory.cast();

        unsafe { inner.as_mut().count = AtomicUsize::new(1) };

        Self { inner }
    }
}

pub fn arc_inner_offset<T>() -> usize {
    offset_of!(ArcInner<T>, inner)
}

pub struct Arc<T: ?Sized> {
    inner: NonNull<ArcInner<T>>,
}

impl<T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<Arc<U>> for Arc<T> {}

unsafe impl<T: ?Sized + Sync> Sync for ArcInner<T> {}
unsafe impl<T: ?Sized + Sync> Send for ArcInner<T> {}

impl<T: ?Sized> Clone for Arc<T> {
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
}

impl<T: ?Sized> Arc<T> {
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

unsafe impl<T: ?Sized + Sync> Sync for Arc<T> {}
unsafe impl<T: ?Sized + Sync> Send for Arc<T> {}

impl<T: ?Sized> Deref for Arc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &self.inner.as_ref().inner }
    }
}

impl<T: ?Sized> Drop for Arc<T> {
    fn drop(&mut self) {
        let inner = unsafe { self.inner.as_ref() };
        let layout = Layout::for_value(&inner.inner);
        if inner.count.fetch_sub(1, SeqCst) == 1 {
            unsafe { drop_in_place(self.inner.as_ptr()) };
            unsafe { KERNEL_ALLOCATOR.deallocate(self.inner.cast(), layout) };
        }
    }
}

pub struct UniqueArc<T: ?Sized> {
    inner: Arc<T>,
}

impl<T> UniqueArc<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}

impl<T> UniqueArc<T> {
    pub fn zeroed() -> Self {
        Self {
            inner: Arc::zeroed(),
        }
    }
}

impl<T: ?Sized> UniqueArc<T> {
    pub unsafe fn as_ptr(&self) -> *const T {
        unsafe { self.inner.as_ptr() }
    }

    pub unsafe fn as_mut(&self) -> *mut T {
        unsafe { self.inner.as_ptr() as _ }
    }
}

impl<T: ?Sized> Deref for UniqueArc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: ?Sized> DerefMut for UniqueArc<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut self.inner.inner.as_mut().inner }
    }
}

impl<T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<UniqueArc<U>> for UniqueArc<T> {}

pub struct ListArc<T: ?Sized, const N: usize> {
    inner: Arc<T>,
}

impl<T: ?Sized, const N: usize> Deref for ListArc<T, N> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: ?Sized, const N: usize> From<UniqueArc<T>> for ListArc<T, N> {
    fn from(value: UniqueArc<T>) -> Self {
        ListArc { inner: value.inner }
    }
}

impl<T: ?Sized, const N: usize> ListArc<T, N> {
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

impl<T: ?Sized + Unsize<U>, U: ?Sized, const N: usize> CoerceUnsized<ListArc<U, N>>
    for ListArc<T, N>
{
}

pub trait LinkedNode<T: ?Sized, const N: usize> {
    fn arc_from_link(link: *mut ListLinks) -> *mut ArcInner<T>;

    fn link_from_arc(arc: *mut ArcInner<T>) -> *mut ListLinks;
}
