use core::{
    cell::Cell,
    marker::{PhantomData, PhantomPinned},
    pin::{self, Pin},
    ptr::NonNull,
    sync::atomic::AtomicBool,
};

use crate::utils::{ArcInner, TreeArc};

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum RbColor {
    Red,
    Black,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Direction {
    Left,
    Right,
}

impl Direction {
    fn opposite(&self) -> Direction {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }
}

pub struct RbLinks {
    parent: Cell<Option<NonNull<RbLinks>>>,
    left: Cell<Option<NonNull<RbLinks>>>,
    right: Cell<Option<NonNull<RbLinks>>>,
    color: Cell<RbColor>,
    owned: AtomicBool,
    phantom_pin: PhantomPinned,
}

impl RbLinks {
    pub const fn new() -> Self {
        Self {
            parent: Cell::new(None),
            left: Cell::new(None),
            right: Cell::new(None),
            color: Cell::new(RbColor::Red),
            owned: AtomicBool::new(false),
            phantom_pin: PhantomPinned,
        }
    }

    unsafe fn addr(self: Pin<&Self>) -> NonNull<RbLinks> {
        let ptr = &raw const *self;

        unsafe { NonNull::new_unchecked(ptr as *mut RbLinks) }
    }

    unsafe fn left(self: Pin<&Self>) -> Option<NonNull<RbLinks>> {
        self.left.get()
    }

    unsafe fn parent(self: Pin<&Self>) -> Option<NonNull<RbLinks>> {
        self.parent.get()
    }

    unsafe fn uncle(self: Pin<&Self>) -> Option<NonNull<RbLinks>> {
        let parent = self.parent.get();
        parent.and_then(|parent| {
            let grandparent = unsafe { parent.as_ref().parent.get() };
            grandparent.and_then(|grandparent| {
                if unsafe { grandparent.as_ref().left.get() == Some(parent) } {
                    unsafe { grandparent.as_ref().right.get() }
                } else {
                    unsafe { grandparent.as_ref().left.get() }
                }
            })
        })
    }

    unsafe fn direction(self: Pin<&Self>) -> Option<Direction> {
        let parent = self.parent.get();
        parent.map(|parent| {
            if unsafe { parent.as_ref().left.get() == Some(self.addr()) } {
                Direction::Left
            } else {
                Direction::Right
            }
        })
    }

    unsafe fn insert_left(self: Pin<&Self>, link: Option<NonNull<RbLinks>>) {
        unsafe {
            if let Some(link) = link {
                link.as_ref().parent.set(Some(self.addr()));
            }
        }

        self.left.set(link);
    }

    unsafe fn pop_left(self: Pin<&Self>) -> Option<NonNull<RbLinks>> {
        let left = self.left.take();
        if let Some(left) = left.as_ref() {
            unsafe {
                left.as_ref().parent.set(None);
            }
        }

        left
    }

    unsafe fn right(self: Pin<&Self>) -> Option<NonNull<RbLinks>> {
        self.right.get()
    }

    unsafe fn insert_right(self: Pin<&Self>, link: Option<NonNull<RbLinks>>) {
        unsafe {
            if let Some(link) = link {
                link.as_ref().parent.set(Some(self.addr()));
            }
        }

        self.right.set(link);
    }

    unsafe fn pop_right(self: Pin<&Self>) -> Option<NonNull<RbLinks>> {
        let right = self.right.take();
        if let Some(right) = right.as_ref() {
            unsafe {
                right.as_ref().parent.set(None);
            }
        }

        right
    }

    /// Swaps the parent, left, and right pointers.
    ///
    /// Does not change colors.
    unsafe fn swap_pointers(self: Pin<&Self>, other: Pin<&Self>) {
        self.parent.swap(&other.parent);
        self.left.swap(&other.left);
        self.right.swap(&other.right);
    }

    unsafe fn take_parent_of(self: Pin<&Self>, other: Pin<&Self>) {
        let parent = other.parent.take();
        if let Some(parent) = parent {
            if unsafe { parent.as_ref().left.get() == Some(other.addr()) } {
                unsafe {
                    Pin::new_unchecked(parent.as_ref()).insert_left(Some(self.addr()));
                }
            } else {
                unsafe {
                    Pin::new_unchecked(parent.as_ref()).insert_right(Some(self.addr()));
                }
            }
        }
    }

    pub fn owned(self: Pin<&Self>) -> Pin<&AtomicBool> {
        unsafe { self.map_unchecked(|links| &links.owned) }
    }

    fn color(&self) -> RbColor {
        self.color.get()
    }

    fn set_color(&self, color: RbColor) {
        self.color.set(color);
    }
}

pub trait RbNode<T: ?Sized, const N: usize> {
    type Key: Ord;

    fn arc_from_link(link: *mut RbLinks) -> *mut ArcInner<T>;

    fn link_from_arc(arc: *mut ArcInner<T>) -> *mut RbLinks;

    fn key_from_link(link: &'_ *mut RbLinks) -> &'_ Self::Key;
}

/// Implemeents link traits on a struct. Does not support generics yet.
#[macro_export]
macro_rules! impl_rblink {
    ($struct:ty,$(let $key:ident:$type:ty = { $index:expr => $field:tt }),*) => {
        $(
        impl $crate::utils::RbNode<$struct, $index> for $struct {
            type Key = $type;

            fn arc_from_link(link: *mut $crate::utils::RbLinks) -> *mut $crate::utils::ArcInner<$struct> {
                let value_ptr = unsafe { (&raw mut *link)
                    .byte_sub(::core::mem::offset_of!($struct, $field))
                    .cast::<$struct>()
                };

                unsafe {
                    value_ptr
                        .byte_sub($crate::utils::arc_inner_offset::<$struct>())
                        .cast::<$crate::utils::ArcInner<$struct>>()
                }
            }

            fn link_from_arc(arc: *mut $crate::utils::ArcInner<$struct>) -> *mut $crate::utils::RbLinks {
                let inner_ptr = unsafe { (&raw mut *arc).byte_add($crate::utils::arc_inner_offset::<$struct>()) };

                unsafe { inner_ptr
                    .byte_add(::core::mem::offset_of!($struct, $field))
                    .cast::<$crate::utils::RbLinks>()
                }
            }

            fn key_from_link(link: &'_ *mut RbLinks) -> &'_ Self::Key {
                let link: *mut RbLinks = *link;
                let value_ptr = unsafe { (&raw const *link)
                    .byte_sub(::core::mem::offset_of!($struct, $field))
                    .cast::<$struct>()
                };

                let key_ptr = unsafe { value_ptr.byte_add(::core::mem::offset_of!($struct, $key)).cast::<Self::Key>() };

                unsafe { key_ptr.as_ref().unwrap() }
            }
        }
        )*
    };
}

struct RbTree<T: RbNode<T, N>, const N: usize = 0> {
    root: Option<NonNull<RbLinks>>,
    _phantom: PhantomData<T>,
}

impl<T: RbNode<T, N>, const N: usize> RbTree<T, N> {
    pub const fn new() -> Self {
        Self {
            root: None,
            _phantom: PhantomData,
        }
    }

    unsafe fn pin_link(link: &*mut RbLinks) -> Pin<&RbLinks> {
        unsafe { Pin::new_unchecked(link.as_ref().unwrap()) }
    }

    unsafe fn pin_nonnull_link(link: &NonNull<RbLinks>) -> Pin<&RbLinks> {
        unsafe { Pin::new_unchecked(link.as_ref()) }
    }

    fn rotate_left(&mut self, link: NonNull<RbLinks>) {
        let link_pin = unsafe { Self::pin_nonnull_link(&link) };
        let parent = unsafe { link_pin.parent() };
        let Some(parent) = parent else {
            return;
        };

        let local_left = unsafe { link_pin.left() };

        // parent -> left child
        let parent_pin = unsafe { Self::pin_nonnull_link(&parent) };
        let grand_parent = unsafe { parent_pin.parent() };

        unsafe {
            parent_pin.insert_right(local_left);
        }
        if grand_parent.is_some() {
            unsafe {
                link_pin.take_parent_of(parent_pin);
            }
        } else {
            link_pin.parent.set(None);
            self.root = Some(link);
        }
        unsafe {
            link_pin.insert_left(Some(parent));
        }
    }

    fn rotate_right(&mut self, link: NonNull<RbLinks>) {
        let link_pin = unsafe { Self::pin_nonnull_link(&link) };
        let parent = unsafe { link_pin.parent() };
        let Some(parent) = parent else {
            return;
        };

        let local_right = unsafe { link_pin.right() };

        // parent -> left child
        let parent_pin = unsafe { Self::pin_nonnull_link(&parent) };
        let grand_parent = unsafe { parent_pin.parent() };

        unsafe {
            parent_pin.insert_left(local_right);
        }
        if grand_parent.is_some() {
            unsafe {
                link_pin.take_parent_of(parent_pin);
            }
        } else {
            link_pin.parent.set(None);
            self.root = Some(link);
        }
        unsafe {
            link_pin.insert_right(Some(parent));
        }
    }

    fn rotate(&mut self, link: NonNull<RbLinks>, direction: Direction) {
        match direction {
            Direction::Left => self.rotate_left(link),
            Direction::Right => self.rotate_right(link),
        }
    }

    fn color_insertion(&mut self, link: NonNull<RbLinks>) {
        let pinned_link = unsafe { Self::pin_nonnull_link(&link) };

        let parent = unsafe { pinned_link.parent() };
        let Some(parent) = parent else {
            pinned_link.set_color(RbColor::Black);
            return;
        };

        let pinned_parent = unsafe { Self::pin_nonnull_link(&parent) };

        if pinned_parent.color() == RbColor::Black {
            pinned_link.set_color(RbColor::Red);
            return;
        }

        let grandparent = unsafe { pinned_parent.parent() };

        let Some(grandparent) = grandparent else {
            pinned_parent.set_color(RbColor::Black);
            return;
        };

        let pinned_grandparent = unsafe { Self::pin_nonnull_link(&grandparent) };

        let uncle = unsafe {
            pinned_link
                .uncle()
                .expect("An uncle should exist at this stage.")
        };
        let pinned_uncle = unsafe { Self::pin_nonnull_link(&uncle) };

        if pinned_uncle.color() == RbColor::Red {
            pinned_parent.set_color(RbColor::Black);
            pinned_uncle.set_color(RbColor::Black);

            self.color_insertion(grandparent);
        } else {
            assert_eq!(pinned_parent.color(), RbColor::Red);
            assert_eq!(pinned_uncle.color(), RbColor::Black);

            let local_direction = unsafe { pinned_link.direction().unwrap() };
            let parent_direction = unsafe { pinned_parent.direction().unwrap() };

            // outer
            if local_direction != parent_direction {
                self.rotate(link, local_direction.opposite());

                // parent becomes current node
                // current node becomes parent
                self.rotate(link, parent_direction.opposite());

                pinned_link.set_color(RbColor::Black);
                pinned_grandparent.set_color(RbColor::Red);
            } else {
                self.rotate(parent, parent_direction.opposite());

                pinned_parent.set_color(RbColor::Black);
                pinned_grandparent.set_color(RbColor::Red);
            }
        }
    }

    fn insert(&mut self, value: TreeArc<T, N>) {
        let new_child_raw_ptr = unsafe { T::link_from_arc(value.into_arc_inner().as_ptr()) };
        let new_child_key = T::key_from_link(&new_child_raw_ptr);
        let new_child = unsafe { NonNull::new_unchecked(new_child_raw_ptr) };

        if self.root.is_none() {
            self.root = Some(new_child);
            self.color_insertion(new_child);
            return;
        }

        let mut current_node = self.root.unwrap();
        loop {
            let current_raw_ptr = current_node.as_ptr();
            let current_raw_pinned = unsafe { Self::pin_link(&current_raw_ptr) };
            let current_key = T::key_from_link(&current_raw_ptr);

            if new_child_key < current_key {
                if let Some(child) = unsafe { current_raw_pinned.left() } {
                    current_node = child;
                } else {
                    unsafe {
                        current_raw_pinned.insert_left(Some(new_child));
                    }
                    self.color_insertion(new_child);
                    break;
                }
            } else {
                if let Some(child) = unsafe { current_raw_pinned.right() } {
                    current_node = child;
                } else {
                    unsafe {
                        current_raw_pinned.insert_right(Some(new_child));
                    }
                    self.color_insertion(new_child);
                    break;
                }
            }
        }
    }
}
