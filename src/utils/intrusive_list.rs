use core::{
    cell::Cell,
    marker::{PhantomData, PhantomPinned},
    pin::Pin,
    ptr::{NonNull, drop_in_place},
    sync::atomic::AtomicBool,
};

use alloc::{alloc::Allocator, boxed::Box};

use crate::utils::{
    Arc,
    arc::{LinkedNode, ListArc, UniqueArc},
};

#[derive(Debug)]
pub struct ListLinks {
    next: Cell<Option<NonNull<ListLinks>>>,
    prev: Cell<Option<NonNull<ListLinks>>>,
    owned: AtomicBool,
    _pin: PhantomPinned,
}

unsafe impl Send for ListLinks {}
unsafe impl Sync for ListLinks {}

impl Default for ListLinks {
    fn default() -> Self {
        Self::new()
    }
}

impl ListLinks {
    pub const fn new() -> Self {
        Self {
            next: Cell::new(None),
            prev: Cell::new(None),
            owned: AtomicBool::new(false),
            _pin: PhantomPinned,
        }
    }

    pub unsafe fn next(self: Pin<&Self>) -> Option<Pin<&Self>> {
        self.next
            .get()
            .map(|next| unsafe { Pin::new_unchecked(next.as_ref()) })
    }

    pub unsafe fn prev(self: Pin<&Self>) -> Option<Pin<&Self>> {
        self.prev
            .get()
            .map(|prev| unsafe { Pin::new_unchecked(prev.as_ref()) })
    }

    pub unsafe fn next_mut(self: Pin<&mut Self>) -> Option<Pin<&mut Self>> {
        self.next
            .get()
            .map(|mut next| unsafe { Pin::new_unchecked(next.as_mut()) })
    }

    pub unsafe fn prev_mut(self: Pin<&mut Self>) -> Option<Pin<&mut Self>> {
        self.prev
            .get()
            .map(|mut prev| unsafe { Pin::new_unchecked(prev.as_mut()) })
    }

    /// Safety: Must ensure that no one else is accessing the list
    pub unsafe fn insert_after(self: Pin<&Self>, link: Pin<&ListLinks>) {
        let next = self.next.get();
        if let Some(next) = next {
            unsafe { next.as_ref() }
                .prev
                .set(Some(NonNull::from_ref(link.get_ref())));
        }

        link.next.set(next);
        link.prev.set(Some(NonNull::from_ref(self.get_ref())));

        self.next.set(Some(NonNull::from_ref(link.get_ref())));
    }

    /// Safety: Must ensure that no one else is accessing the list
    pub unsafe fn insert_before(self: Pin<&Self>, link: Pin<&ListLinks>) {
        let prev = self.prev.get();

        if let Some(prev) = prev {
            unsafe { prev.as_ref() }
                .next
                .set(Some(NonNull::from_ref(link.get_ref())));
        }

        link.next.set(Some(NonNull::from_ref(self.get_ref())));
        link.prev.set(prev);

        self.prev.set(Some(NonNull::from_ref(link.get_ref())));
    }

    /// Safety: Must ensure that no one else is accessing the list
    pub unsafe fn remove(self: Pin<&Self>) {
        let prev = self.prev.get();
        let next = self.next.get();
        if let Some(prev) = prev {
            unsafe {
                prev.as_ref().next.set(next);
            }
        }
        if let Some(next) = next {
            unsafe {
                next.as_ref().prev.set(prev);
            }
        }
        self.next.set(None);
        self.prev.set(None);
    }

    pub fn is_linked(self: Pin<&Self>) -> bool {
        self.prev.get().is_some() && self.next.get().is_some()
    }
}

impl Drop for ListLinks {
    fn drop(&mut self) {
        unsafe {
            Pin::new_unchecked(self).into_ref().remove();
        };
    }
}

/// Implemeents link traits on a struct. Does not support generics yet.
#[macro_export]
macro_rules! impl_link {
    ($struct:ty,$($index:expr => $field:tt),*) => {
        $(
        impl $crate::utils::LinkedNode<$struct, $index> for $struct {
            fn arc_from_link(link: *mut crate::utils::ListLinks) -> *mut crate::utils::ArcInner<$struct> {
                let value_ptr = unsafe { (&raw mut *link)
                    .byte_sub(::core::mem::offset_of!($struct, $field))
                    .cast::<$struct>()
                };

                unsafe {
                    value_ptr
                        .byte_sub(crate::utils::arc_inner_offset::<$struct>())
                        .cast::<crate::utils::ArcInner<$struct>>()
                }
            }

            fn link_from_arc(arc: *mut crate::utils::ArcInner<$struct>) -> *mut crate::utils::ListLinks {
                let inner_ptr = unsafe { (&raw mut *arc).byte_add(crate::utils::arc_inner_offset::<$struct>()) };

                unsafe { inner_ptr
                    .byte_add(::core::mem::offset_of!($struct, $field))
                    .cast::<crate::utils::ListLinks>()
                }
            }
        }
        )*
    };
}

pub struct List<T: LinkedNode<T, N>, const N: usize = 0> {
    head: UniqueArc<ListLinks>,
    _phantom: PhantomData<T>,
}

impl<T: LinkedNode<T, N>, const N: usize> Drop for List<T, N> {
    fn drop(&mut self) {
        while let Some(node) = self.remove_front() {
            drop(node);
        }
    }
}

impl<T: LinkedNode<T, N>, const N: usize> List<T, N> {
    pub fn new() -> Self {
        let mut s = Self {
            head: UniqueArc::new(ListLinks::new()),
            _phantom: PhantomData,
        };

        s.init();

        s
    }

    fn init(&mut self) {
        let head_ref = NonNull::from_ref(&self.head);

        unsafe {
            let head_ptr = Some(NonNull::new(head_ref.as_ref().as_mut()).unwrap());
            self.head.next.set(head_ptr);
            self.head.prev.set(head_ptr);
        }
    }

    pub fn push_front(&mut self, item: ListArc<T, N>) {
        let inner = unsafe { ListArc::into_arc_inner(item).as_ptr() };
        let link = unsafe { Pin::new_unchecked(T::link_from_arc(inner).as_ref().unwrap()) };

        unsafe {
            self.sentinel().insert_after(link);
        }
    }

    pub fn push_back(&mut self, item: ListArc<T, N>) {
        let inner = unsafe { ListArc::into_arc_inner(item).as_ptr() };
        let link = unsafe { Pin::new_unchecked(T::link_from_arc(inner).as_ref().unwrap()) };

        unsafe {
            self.sentinel().insert_before(link);
        }
    }

    pub fn remove_front(&mut self) -> Option<ListArc<T, N>> {
        let next_ptr = unsafe { self.sentinel().next() };

        if let Some(link) = next_ptr {
            unsafe {
                link.remove();

                let ptr = &raw const *link.as_ref();
                let arc_inner = T::arc_from_link(ptr as *mut ListLinks);

                Some(ListArc::from_arc_inner(NonNull::new_unchecked(arc_inner)))
            }
        } else {
            None
        }
    }

    pub fn remove_back(&mut self) -> Option<ListArc<T, N>> {
        let prev_ptr = unsafe { self.sentinel().prev() };

        if let Some(link) = prev_ptr {
            unsafe {
                link.remove();

                let ptr = &raw const *link.as_ref();
                let arc_inner = T::arc_from_link(ptr as *mut ListLinks);

                Some(ListArc::from_arc_inner(NonNull::new_unchecked(arc_inner)))
            }
        } else {
            None
        }
    }

    fn sentinel(&self) -> Pin<&ListLinks> {
        unsafe { Pin::new_unchecked(&*self.head) }
    }

    fn sentinel_addr(&self) -> usize {
        (&raw const *self.head).addr()
    }

    pub fn is_empty(&self) -> bool {
        self.head.next.get().unwrap().addr().get() == self.sentinel_addr()
    }

    pub fn iter(&self) -> ListCursor<'_, T, N> {
        ListCursor {
            ptr: self.head.next.get(),
            list: self,
        }
    }

    pub unsafe fn iter_at(&self, node: &Arc<T>) -> ListCursor<'_, T, N> {
        let link = T::link_from_arc(unsafe { node.as_inner_ptr() } as *mut _);

        ListCursor {
            ptr: NonNull::new(link),
            list: self,
        }
    }

    pub unsafe fn remove_at(&mut self, node: &Arc<T>) -> ListArc<T, N> {
        let ptr = T::link_from_arc(unsafe { node.as_inner_ptr() } as *mut _);
        let link = unsafe { Pin::new_unchecked(ptr.as_ref().unwrap()) };

        unsafe {
            link.remove();
        }

        unsafe { ListArc::from_arc_inner(NonNull::new_unchecked(T::arc_from_link(ptr))) }
    }
}

pub struct ListCursor<'a, T: LinkedNode<T, N>, const N: usize = 0> {
    list: &'a List<T, N>,
    ptr: Option<NonNull<ListLinks>>,
}

impl<'a, T: LinkedNode<T, N>, const N: usize> Iterator for ListCursor<'a, T, N> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.ptr.map(|ptr| {
            let link: Pin<&ListLinks> = unsafe { Pin::new_unchecked(ptr.as_ref()) };

            let ptr = &raw const *link.as_ref();
            let arc_inner = T::arc_from_link(ptr as *mut ListLinks);

            unsafe { (*arc_inner).as_raw().as_ref().unwrap() }
        });

        self.ptr = self.ptr.and_then(|ptr| unsafe { ptr.as_ref().next.get() });
        if self
            .ptr
            .is_none_or(|ptr| ptr.addr().get() == self.list.sentinel_addr())
        {
            self.ptr = None;
        }

        item
    }
}

impl<'a, T: LinkedNode<T, N>, const N: usize> ListCursor<'a, T, N> {
    pub fn get(&self) -> Option<&T> {
        self.ptr.map(|ptr| {
            let link: Pin<&ListLinks> = unsafe { Pin::new_unchecked(ptr.as_ref()) };

            let ptr = &raw const *link.as_ref();
            let arc_inner = T::arc_from_link(ptr as *mut ListLinks);

            unsafe { (*arc_inner).as_raw().as_ref().unwrap() }
        })
    }
}
