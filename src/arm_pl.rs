use core::fmt::Write;

use crate::utils::CoreLock;

const ARMPL_BASE: *mut u8 = 0x9000000 as _;

pub static ARM_PL: CoreLock<ArmPl> = CoreLock::new(ArmPl {});

fn putchar_early(c: char) {
    while unsafe { ARMPL_BASE.byte_offset(0x18).read_volatile() } & (1 << 5) != 0 {}

    unsafe { ARMPL_BASE.write_volatile(c as u8) };
}

pub struct ArmPl {}

unsafe impl Send for ArmPl {}
unsafe impl Sync for ArmPl {}

impl ArmPl {
    pub fn early_printk_str(&self, s: &str) {
        for c in s.chars() {
            putchar_early(c);
        }
    }
}

impl Write for ArmPl {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.early_printk_str(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! early_printk {
    ($($x:expr),*) => {
        use ::core::fmt::Write;
        let _ = ::core::write!($crate::arm_pl::ARM_PL.lock(), $($x,)*);
    };
}
