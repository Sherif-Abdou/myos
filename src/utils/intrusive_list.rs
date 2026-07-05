use core::{
    cell::Cell, marker::{PhantomData, PhantomPinned}, pin::Pin, ptr::{NonNull, drop_in_place},
};

use alloc::{alloc::Allocator, boxed::Box};

#[derive(Debug)]
pub struct ListLinks {
    next: Cell<Option<NonNull<ListLinks>>>,
    prev: Cell<Option<NonNull<ListLinks>>>,
    _pin: PhantomPinned,
}

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
        if let Some(mut prev) = prev {
            unsafe {
                prev.as_mut().next.set(next);
            }
        }
        if let Some(mut next) = next {
            unsafe {
                next.as_mut().prev.set(prev);
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

pub trait LinkedNode<const N: usize> {
    type Inner;

    fn get_link(self: Pin<&Self>) -> Pin<&ListLinks>;

    fn get_link_mut(self: Pin<&mut Self>) -> Pin<&mut ListLinks>;

    fn get_from_link(link: Pin<&ListLinks>) -> Pin<&Self::Inner>;

    fn get_from_link_mut(link: Pin<&mut ListLinks>) -> Pin<&mut Self::Inner>;

    fn link_offset() -> usize;
}

impl<T: LinkedNode<N>, A: Allocator, const N: usize> LinkedNode<N> for Pin<Box<T, A>> {
    type Inner = T::Inner;

    fn get_link(self: Pin<&Self>) -> Pin<&ListLinks> {
        let pin = self.get_ref().as_ref();

        pin.get_link()
    }

    fn get_link_mut(self: Pin<&mut Self>) -> Pin<&mut ListLinks> {
        let pin = self.get_mut().as_mut();

        pin.get_link_mut()
    }

    fn get_from_link(link: Pin<&ListLinks>) -> Pin<&Self::Inner> {
        unsafe {
            link.map_unchecked(|link| {
                (&raw const *link)
                    .byte_sub(Self::link_offset())
                    .cast::<Self::Inner>()
                    .as_ref()
                    .unwrap()
            })
        }
    }

    fn get_from_link_mut(link: Pin<&mut ListLinks>) -> Pin<&mut Self::Inner> {
        unsafe {
            link.map_unchecked_mut(|link| {
                (&raw mut *link)
                    .byte_sub(Self::link_offset())
                    .cast::<Self::Inner>()
                    .as_mut()
                    .unwrap()
            })
        }
    }

    fn link_offset() -> usize {
        T::link_offset()
    }
}

/// Implemeents link traits on a struct. Does not support generics yet.
#[macro_export]
macro_rules! impl_link {
    ($struct:ty,$($index:expr => $field:tt),*) => {
        $(
        impl $crate::utils::LinkedNode<$index> for $struct {
            type Inner = Self;

            fn get_link(self: ::core::pin::Pin<&Self>) -> ::core::pin::Pin<&$crate::utils::ListLinks> {
                unsafe { self.map_unchecked(|pin| &pin.$field) }
            }

            fn get_link_mut(self: ::core::pin::Pin<&mut Self>) -> ::core::pin::Pin<&mut $crate::utils::ListLinks> {
                unsafe { self.map_unchecked_mut(|pin| &mut pin.$field) }
            }

            fn get_from_link(link: ::core::pin::Pin<&$crate::utils::ListLinks>) -> ::core::pin::Pin<&Self::Inner> {
                unsafe {
                    link.map_unchecked(|link| {
                        (&raw const *link)
                            .byte_sub(::core::mem::offset_of!(Self, $field))
                            .cast::<Self>()
                            .as_ref()
                            .unwrap()
                    })
                }
            }

            fn get_from_link_mut(link: ::core::pin::Pin<&mut $crate::utils::ListLinks>) -> ::core::pin::Pin<&mut Self::Inner> {
                unsafe {
                    link.map_unchecked_mut(|link| {
                        (&raw mut *link)
                            .byte_sub(::core::mem::offset_of!(Self, $field))
                            .cast::<Self>()
                            .as_mut()
                            .unwrap()
                    })
                }
            }

            fn link_offset() -> usize {
                ::core::mem::offset_of!(Self, $field)
            }
        }
        )*
    };
}

pub struct List<T: LinkedNode<N>, const N: usize = 0> {
    head: ListLinks,
    _phantom: PhantomData<T>,
}

impl<T: LinkedNode<N>, const N: usize> List<T, N> {
    pub const fn new() -> Self {
        Self {
            head: ListLinks::new(),
            _phantom: PhantomData,
        }
    }

    pub fn init(mut self: Pin<&mut Self>) {
        let head_ref = NonNull::from_ref(&self.head);

        self.as_mut().head.next.set(Some(head_ref));
        self.as_mut().head.prev.set(Some(head_ref));
    }

    pub fn push_front(self: Pin<&mut Self>, item: Pin<&T>) {
        let link = T::get_link(item);
        let head = unsafe { self.map_unchecked_mut(|list| &mut list.head) };

        // Safety: Having mutable access to this list implies synchronization of the linked list.
        unsafe {
            head.into_ref().insert_after(link);
        }
    }

    pub fn push_back(self: Pin<&mut Self>, item: Pin<&T>) {
        let link = T::get_link(item);
        let head = unsafe { self.map_unchecked_mut(|list| &mut list.head) };

        // Safety: Having mutable access to this list implies synchronization of the linked list.
        unsafe {
            head.into_ref().insert_before(link);
        }
    }

    pub fn remove_front(self: Pin<&mut Self>) {
        let head = unsafe { self.map_unchecked_mut(|list| &mut list.head) };

        // Safety: Having mutable access to this list implies synchronization of the linked list.
        unsafe { head.as_ref().next().map(|v| v.remove()) };
    }

    pub fn remove_back(self: Pin<&mut Self>) {
        let head = unsafe { self.map_unchecked_mut(|list| &mut list.head) };

        // Safety: Having mutable access to this list implies synchronization of the linked list.
        unsafe { head.as_ref().prev().map(|v| v.remove()) };
    }

    fn head_addr(self: Pin<&Self>) -> usize {
        (&raw const self.head).addr()
    }

    pub fn iter(self: Pin<&Self>) -> ListIterator<'_, T, N> {
        ListIterator {
            ptr: self.head.next.get(),
            list: self,
        }
    }

    pub fn iter_mut(self: Pin<&mut Self>) -> ListIteratorMut<'_, T, N> {
        ListIteratorMut {
            ptr: self.head.next.get(),
            list: self,
        }
    }
}

pub struct ListIterator<'a, T: LinkedNode<N>, const N: usize = 0> {
    list: Pin<&'a List<T, N>>,
    ptr: Option<NonNull<ListLinks>>,
}

impl<'a, T: LinkedNode<N>, const N: usize> Iterator for ListIterator<'a, T, N> {
    type Item = Pin<&'a T::Inner>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.ptr.map(|ptr| {
            let pin: Pin<&ListLinks> = unsafe { Pin::new_unchecked(ptr.as_ref()) };
            T::get_from_link(pin)
        });

        self.ptr = self.ptr.and_then(|ptr| unsafe { ptr.as_ref().next.get() });
        if self
            .ptr
            .is_none_or(|ptr| ptr.addr().get() == self.list.head_addr())
        {
            self.ptr = None;
        }

        item
    }
}

pub struct ListIteratorMut<'a, T: LinkedNode<N>, const N: usize = 0> {
    list: Pin<&'a mut List<T, N>>,
    ptr: Option<NonNull<ListLinks>>,
}

impl<'a, T: LinkedNode<N>, const N: usize> ListIteratorMut<'a, T, N> {
    /// Safety: This list must be the only owner of the element to be dropped.
    pub unsafe fn remove_and_drop(&mut self) {
        if let Some(mut ptr) = self.ptr {
            // Safety: Every element of this list is assumed to be pinned.
            let pin: Pin<&mut ListLinks> = unsafe { Pin::new_unchecked(ptr.as_mut()) };
            unsafe { pin.as_ref().remove(); }

            unsafe { drop_in_place(ptr.as_ptr()) };
        }
    }
}

impl<'a, T: LinkedNode<N>, const N: usize> Iterator for ListIteratorMut<'a, T, N> {
    type Item = Pin<&'a mut T::Inner>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.ptr.map(|mut ptr| {
            let pin: Pin<&mut ListLinks> = unsafe { Pin::new_unchecked(ptr.as_mut()) };
            T::get_from_link_mut(pin)
        });

        self.ptr = self.ptr.and_then(|ptr| unsafe { ptr.as_ref().next.get() });
        if self
            .ptr
            .is_none_or(|ptr| ptr.addr().get() == self.list.as_ref().head_addr())
        {
            self.ptr = None;
        }

        item
    }
}
