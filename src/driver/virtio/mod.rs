mod virtio_blk;

pub use virtio_blk::VirtioBlkDriver;

#[repr(C, align(16))]
#[derive(Default, PartialEq, Eq)]
pub(crate) struct VirtqDescriptor {
    pub(crate) addr: usize,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16
}

#[repr(C)]
pub(crate) struct VirtqAvailable<const N: usize> {
    pub(crate) flags: u16,
    pub(crate) idx: u16,
    pub(crate) ring: [u16; N],
}

impl<const N: usize> VirtqAvailable<N> {
    pub fn addr(&self) -> usize {
        (&raw const *self).addr()
    }
}

impl<const N: usize> VirtqAvailable<N> {
    pub const fn zeroed() -> Self {
        unsafe { core::mem::zeroed() }
    }
}

#[repr(C)]
pub(crate) struct VirtqUsed<const N: usize> {
    pub(crate) flags: u16,
    pub(crate) idx: u16,
    pub(crate) ring: [VirtqUsedElem; N],
}

impl<const N: usize> VirtqUsed<N> {
    pub fn addr(&self) -> usize {
        (&raw const *self).addr()
    }
}

impl<const N: usize> VirtqUsed<N> {
    pub const fn zeroed() -> Self {
        unsafe { core::mem::zeroed() }
    }
}

#[repr(C)]
#[derive(Default, PartialEq, Eq)]
pub(crate) struct VirtqUsedElem {
    pub(crate) id: u32,
    pub(crate) len: u32,
}

pub(crate) struct Virtq<const N: usize> {
    pub(crate) available: VirtqAvailable<N>,
    pub(crate) used: VirtqUsed<N>,
}

impl<const N: usize> Virtq<N> {
    pub const fn zeroed() -> Self {
        Self {
            available: VirtqAvailable::zeroed(),
            used: VirtqUsed::zeroed(),
        }
    }
}
