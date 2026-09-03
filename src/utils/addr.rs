use core::ptr::NonNull;

pub struct PhysAddr(usize);

impl PhysAddr {
    pub const fn get(&self) -> usize {
        self.0
    }

    pub const fn from_raw_addr(address: usize) -> Self {
        Self(address)
    }
}

const PHYS_ADDR_MASK: usize = 0x7fffffffff;

impl<T: ?Sized> From<*const T> for PhysAddr {
    fn from(value: *const T) -> Self {
        PhysAddr(value.addr() & PHYS_ADDR_MASK)
    }
}

impl<T: ?Sized> From<*mut T> for PhysAddr {
    fn from(value: *mut T) -> Self {
        PhysAddr(value.addr() & PHYS_ADDR_MASK)
    }
}

impl<T: ?Sized> From<NonNull<T>> for PhysAddr {
    fn from(value: NonNull<T>) -> Self {
        PhysAddr(value.addr().get() & PHYS_ADDR_MASK)
    }
}

impl<T: ?Sized> From<&T> for PhysAddr {
    fn from(value: &T) -> Self {
        PhysAddr((&raw const *value).addr() & PHYS_ADDR_MASK)
    }
}

impl<T: ?Sized> From<&mut T> for PhysAddr {
    fn from(value: &mut T) -> Self {
        PhysAddr((&raw const *value).addr() & PHYS_ADDR_MASK)
    }
}
