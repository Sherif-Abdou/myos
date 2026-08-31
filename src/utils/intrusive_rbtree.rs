use core::{
    cell::Cell,
    marker::{PhantomData, PhantomPinned},
    pin::{self, Pin},
    ptr::NonNull,
    sync::atomic::AtomicBool,
};

use crate::{
    utils::{ArcInner, TreeArc},
    write_sysreg,
};

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

    unsafe fn child(self: Pin<&Self>, direction: Direction) -> Option<NonNull<RbLinks>> {
        unsafe {
            match direction {
                Direction::Left => self.left(),
                Direction::Right => self.right(),
            }
        }
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

    unsafe fn sibling(self: Pin<&Self>) -> Option<NonNull<RbLinks>> {
        let parent = self.parent.get();
        parent.and_then(|parent| {
            if unsafe { parent.as_ref().left.get() == Some(self.addr()) } {
                unsafe { parent.as_ref().right.get() }
            } else {
                unsafe { parent.as_ref().left.get() }
            }
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
    ///
    /// Returns a pointer to a new root if the root has been changed by this swap.
    unsafe fn swap_pointers(self: Pin<&Self>, other: Pin<&Self>) -> Option<NonNull<RbLinks>> {
        if unsafe { self.parent() == Some(other.addr()) } {
            let local_direction = unsafe { self.direction().unwrap() };
            let mut new_root = None;
            if let Some(parent) = unsafe { other.parent() } {
                let parent_direction = unsafe { other.direction().unwrap() };
                let pinned_parent = unsafe { Pin::new_unchecked(parent.as_ref()) };
                unsafe {
                    pinned_parent.insert_child(parent_direction, Some(self.addr()));
                }
            } else {
                self.parent.set(None);
                new_root = Some(unsafe { self.addr() });
            }
            unsafe {
                other.parent.set(Some(self.addr()));
                let old_left = self.left();
                let old_right = self.right();
                let old_sibling = other.child(local_direction.opposite());

                self.insert_child(local_direction, Some(other.addr()));
                self.insert_child(local_direction.opposite(), old_sibling);
                other.insert_left(old_left);
                other.insert_right(old_right);
            }

            return new_root;
        } else if unsafe { other.parent() == Some(self.addr()) } {
            return unsafe { other.swap_pointers(self) };
        }

        self.parent.swap(&other.parent);
        self.left.swap(&other.left);
        self.right.swap(&other.right);
        if let Some(left) = self.left.get() {
            unsafe {
                (*left.as_ptr()).parent.set(Some(self.addr()));
            }
        }
        if let Some(right) = self.right.get() {
            unsafe {
                (*right.as_ptr()).parent.set(Some(self.addr()));
            }
        }
        if let Some(left) = other.left.get() {
            unsafe {
                (*left.as_ptr()).parent.set(Some(other.addr()));
            }
        }
        if let Some(right) = other.right.get() {
            unsafe {
                (*right.as_ptr()).parent.set(Some(other.addr()));
            }
        }

        if let Some(parent) = unsafe { self.parent() } {
            unsafe {
                if let Some(left) = (*parent.as_ptr()).left.get()
                    && (left == other.addr())
                {
                    (*parent.as_ptr()).left.set(Some(self.addr()));
                }
            }

            unsafe {
                if let Some(right) = (*parent.as_ptr()).right.get()
                    && (right == other.addr())
                {
                    (*parent.as_ptr()).right.set(Some(self.addr()));
                }
            }
        }

        if let Some(parent) = unsafe { other.parent() } {
            unsafe {
                if let Some(left) = (*parent.as_ptr()).left.get()
                    && (left == self.addr())
                {
                    (*parent.as_ptr()).left.set(Some(other.addr()));
                }
            }

            unsafe {
                if let Some(right) = (*parent.as_ptr()).right.get()
                    && (right == self.addr())
                {
                    (*parent.as_ptr()).right.set(Some(other.addr()));
                }
            }
        }

        if unsafe { self.parent().is_none() } {
            Some(unsafe { self.addr() })
        } else if unsafe { other.parent().is_none() } {
            Some(unsafe { other.addr() })
        } else {
            None
        }
    }

    unsafe fn take_parent_of(self: Pin<&Self>, other: Pin<&Self>) {
        let parent = other.parent.take();
        unsafe {
            self.update_parent(other.addr(), parent);
        }
    }

    unsafe fn insert_child(
        self: Pin<&Self>,
        direction: Direction,
        child: Option<NonNull<RbLinks>>,
    ) {
        unsafe {
            match direction {
                Direction::Left => self.insert_left(child),
                Direction::Right => self.insert_right(child),
            }
        }
    }

    fn update_parent(
        self: Pin<&Self>,
        old_child: NonNull<RbLinks>,
        parent: Option<NonNull<RbLinks>>,
    ) {
        if let Some(parent) = parent {
            if unsafe { parent.as_ref().left.get() == Some(old_child) } {
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

    unsafe fn remove(self: Pin<&Self>) {
        assert!(self.left.get().is_none());
        assert!(self.right.get().is_none());

        let parent = self.parent.take();
        if let Some(parent) = parent {
            if unsafe { parent.as_ref().left.get() == Some(self.addr()) } {
                unsafe {
                    Pin::new_unchecked(parent.as_ref()).insert_left(None);
                }
            } else {
                unsafe {
                    Pin::new_unchecked(parent.as_ref()).insert_right(None);
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

    fn inorder_successor(&mut self, link: NonNull<RbLinks>) -> Option<NonNull<RbLinks>> {
        let pinned_link = unsafe { Self::pin_nonnull_link(&link) };
        if let Some(right) = unsafe { pinned_link.right() } {
            let mut cur_link = right;
            while let Some(left) = unsafe { Self::pin_nonnull_link(&cur_link).left() } {
                cur_link = left;
            }
            return Some(cur_link);
        } else {
            let mut cur_link = link;
            let mut cur_parent = unsafe { Self::pin_nonnull_link(&cur_link).parent() };
            while let Some(parent) = cur_parent {
                let parent_pin = unsafe { Self::pin_nonnull_link(&parent) };
                if unsafe { parent_pin.left() } == Some(cur_link) {
                    break;
                } else {
                    cur_link = parent;
                    cur_parent = unsafe { Self::pin_nonnull_link(&cur_link).parent() };
                }
            }

            if let Some(parent) = cur_parent {
                return Some(parent);
            }
        }
        None
    }

    fn inorder_predecessor(&mut self, link: NonNull<RbLinks>) -> Option<NonNull<RbLinks>> {
        let pinned_link = unsafe { Self::pin_nonnull_link(&link) };
        if let Some(left) = unsafe { pinned_link.left() } {
            let mut cur_link = left;
            while let Some(right) = unsafe { Self::pin_nonnull_link(&cur_link).right() } {
                cur_link = right;
            }
            return Some(cur_link);
        } else {
            let mut cur_link = link;
            let mut cur_parent = unsafe { Self::pin_nonnull_link(&cur_link).parent() };
            while let Some(parent) = cur_parent {
                let parent_pin = unsafe { Self::pin_nonnull_link(&parent) };
                if unsafe { parent_pin.right() } == Some(cur_link) {
                    break;
                } else {
                    cur_link = parent;
                    cur_parent = unsafe { Self::pin_nonnull_link(&cur_link).parent() };
                }
            }

            if let Some(parent) = cur_parent {
                return Some(parent);
            }
        }
        None
    }

    fn remove(&mut self, link: NonNull<RbLinks>) {
        let pinned_link = unsafe { Self::pin_nonnull_link(&link) };

        let mut right = unsafe { pinned_link.right() };

        if unsafe { pinned_link.left().is_some() } && right.is_some() {
            let successor = self
                .inorder_successor(link)
                .expect("If node has two children, it must have in order successor.");
            let pinned_successor = unsafe { Self::pin_nonnull_link(&successor) };

            unsafe {
                if let Some(root) = pinned_link.swap_pointers(pinned_successor) {
                    self.root = Some(root);
                }
            }

            right = unsafe { pinned_link.right() };
        }

        if let Some(right) = right {
            let pinned_right = unsafe { Self::pin_nonnull_link(&right) };

            unsafe {
                if let Some(root) = pinned_link.swap_pointers(pinned_right) {
                    self.root = Some(root);
                }
            }
            pinned_right.set_color(RbColor::Black);
            unsafe {
                pinned_link.remove();
            }
            return;
        } else if let Some(left) = unsafe { pinned_link.left() } {
            let pinned_left = unsafe { Self::pin_nonnull_link(&left) };

            unsafe {
                if let Some(root) = pinned_link.swap_pointers(pinned_left) {
                    self.root = Some(root);
                }
            }
            pinned_left.set_color(RbColor::Black);
            unsafe {
                pinned_link.remove();
            }
            return;
        }

        if self.root == Some(link) {
            unsafe {
                pinned_link.remove();
            }
            self.root = None;
            return;
        }

        if pinned_link.color() == RbColor::Red {
            unsafe {
                pinned_link.remove();
            }
            return;
        }

        let direction = unsafe { pinned_link.direction().unwrap() };
        let parent = unsafe { pinned_link.parent().unwrap() };

        unsafe {
            pinned_link.remove();
        }

        self.color_remove(parent, direction);
    }

    fn color_remove(&mut self, parent: NonNull<RbLinks>, mut direction: Direction) {
        macro_rules! pin_nonnull {
            ($link:expr) => {
                unsafe { Self::pin_nonnull_link(&$link) }
            };
        }
        let mut outer_parent = Some(parent);

        while let Some(parent) = outer_parent {
            let pinned_parent = pin_nonnull!(&parent);
            let sibling = unsafe {
                pinned_parent
                    .child(direction.opposite())
                    .expect("There must be a sibling in this scenario.")
            };
            let pinned_sibling = pin_nonnull!(&sibling);
            let close_nephew = unsafe { pinned_sibling.child(direction) };
            let distant_nephew = unsafe { pinned_sibling.child(direction.opposite()) };

            if pinned_sibling.color() == RbColor::Red {
                self.rotate(sibling, direction);
                pinned_parent.set_color(RbColor::Red);
                pinned_sibling.set_color(RbColor::Black);
                let sibling = close_nephew.unwrap();
                let pinned_sibling = pin_nonnull!(sibling);
                let close_nephew = unsafe { pinned_sibling.child(direction) };
                let distant_nephew = unsafe { pinned_sibling.child(direction.opposite()) };

                if let Some(distant_nephew) = distant_nephew
                    && pin_nonnull!(distant_nephew).color() == RbColor::Red
                {
                    self.rotate(sibling, direction);
                    pinned_sibling.set_color(pinned_parent.color());
                    pinned_parent.set_color(RbColor::Black);
                    pin_nonnull!(distant_nephew).set_color(RbColor::Black);
                    return;
                }

                if let Some(close_nephew) = close_nephew
                    && pin_nonnull!(close_nephew).color() == RbColor::Red
                {
                    self.rotate(close_nephew, direction.opposite());
                    pinned_sibling.set_color(RbColor::Red);
                    pin_nonnull!(&close_nephew).set_color(RbColor::Black);
                    let distant_nephew = sibling;
                    let sibling = close_nephew;

                    self.rotate(sibling, direction);
                    pin_nonnull!(sibling).set_color(pinned_parent.color());
                    pinned_parent.set_color(RbColor::Black);
                    pin_nonnull!(distant_nephew).set_color(RbColor::Black);
                    return;
                }

                pinned_parent.set_color(RbColor::Black);
                pinned_sibling.set_color(RbColor::Red);
                return;
            }

            if let Some(distant_nephew) = distant_nephew
                && pin_nonnull!(distant_nephew).color() == RbColor::Red
            {
                self.rotate(sibling, direction);
                pinned_sibling.set_color(pinned_parent.color());
                pinned_parent.set_color(RbColor::Black);
                pin_nonnull!(distant_nephew).set_color(RbColor::Black);
                return;
            }

            if let Some(close_nephew) = close_nephew
                && pin_nonnull!(close_nephew).color() == RbColor::Red
            {
                self.rotate(close_nephew, direction.opposite());
                pinned_sibling.set_color(RbColor::Red);
                pin_nonnull!(&close_nephew).set_color(RbColor::Black);
                let distant_nephew = sibling;
                let sibling = close_nephew;

                self.rotate(sibling, direction);
                pin_nonnull!(sibling).set_color(pinned_parent.color());
                pinned_parent.set_color(RbColor::Black);
                pin_nonnull!(distant_nephew).set_color(RbColor::Black);
                return;
            }

            if pinned_parent.color() == RbColor::Red {
                pinned_sibling.set_color(RbColor::Red);
                pinned_parent.set_color(RbColor::Black);
                return;
            }

            pinned_sibling.set_color(RbColor::Red);
            direction = unsafe {
                // Doesn't matter if there's no parent
                pinned_parent.direction().unwrap_or(Direction::Left)
            };
            outer_parent = unsafe { pinned_parent.parent() };
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
