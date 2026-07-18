pub struct MMIO {
    base: *mut u8,
    size: usize,
}

// MMIO is expected to be synchronized through device memory rules in arm.
unsafe impl Sync for MMIO {}
unsafe impl Send for MMIO {}

impl MMIO {
    pub const fn new(physical_base: usize, size: usize) -> Self {
        Self {
            base: (physical_base | 0xffffff80_00000000) as *mut u8,
            size,
        }
    }

    pub unsafe fn read_u8(&self, offset: usize) -> u8 {
        unsafe { self.base.byte_add(offset).cast::<u8>().read_volatile() }
    }

    pub unsafe fn read_u16(&self, offset: usize) -> u16 {
        unsafe { self.base.byte_add(offset).cast::<u16>().read_volatile() }
    }

    pub unsafe fn read_u32(&self, offset: usize) -> u32 {
        unsafe { self.base.byte_add(offset).cast::<u32>().read_volatile() }
    }

    pub unsafe fn read_u64(&self, offset: usize) -> u64 {
        unsafe { self.base.byte_add(offset).cast::<u64>().read_volatile() }
    }

    pub unsafe fn write_u8(&self, value: u8, offset: usize) {
        unsafe {
            self.base
                .byte_add(offset)
                .cast::<u8>()
                .write_volatile(value)
        }
    }

    pub unsafe fn write_u16(&self, value: u16, offset: usize) {
        unsafe {
            self.base
                .byte_add(offset)
                .cast::<u16>()
                .write_volatile(value)
        }
    }

    pub unsafe fn write_u32(&self, value: u32, offset: usize) {
        unsafe {
            self.base
                .byte_add(offset)
                .cast::<u32>()
                .write_volatile(value)
        }
    }

    pub unsafe fn write_u64(&self, value: u64, offset: usize) {
        unsafe {
            self.base
                .byte_add(offset)
                .cast::<u64>()
                .write_volatile(value)
        }
    }

    pub unsafe fn modify_u8(&self, offset: usize, rmw: impl FnOnce(u8) -> u8) {
        unsafe {
            let before = self.read_u8(offset);
            self.write_u8(rmw(before), offset);
        }
    }

    pub unsafe fn modify_u16(&self, offset: usize, rmw: impl FnOnce(u16) -> u16) {
        unsafe {
            let before = self.read_u16(offset);
            self.write_u16(rmw(before), offset);
        }
    }

    pub unsafe fn modify_u32(&self, offset: usize, rmw: impl FnOnce(u32) -> u32) {
        unsafe {
            let before = self.read_u32(offset);
            self.write_u32(rmw(before), offset);
        }
    }

    pub unsafe fn modify_u64(&self, offset: usize, rmw: impl FnOnce(u64) -> u64) {
        unsafe {
            let before = self.read_u64(offset);
            self.write_u64(rmw(before), offset);
        }
    }
}
