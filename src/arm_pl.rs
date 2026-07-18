use core::{fmt::Write, ptr::NonNull};

use crate::{dtb::FdtNode, utils::CoreLock};

pub static EARLYCONSOLE_ARM_PL: CoreLock<ArmPl> = CoreLock::new(ArmPl { ptr: None });

pub struct ArmPl {
    ptr: Option<NonNull<u8>>,
}

unsafe impl Send for ArmPl {}
unsafe impl Sync for ArmPl {}

impl ArmPl {
    pub fn early_printk_str(&self, s: &str) {
        if self.ptr.is_some() {
            for c in s.chars() {
                self.early_putchar(c);
            }
        }
    }

    pub fn early_putchar(&self, c: char) {
        if let Some(ptr) = self.ptr {
            while unsafe { ptr.byte_offset(0x18).read_volatile() } & (1 << 5) != 0 {}

            unsafe { ptr.write_volatile(c as u8) };
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
        {
        use ::core::fmt::Write;
        let _ = ::core::write!($crate::arm_pl::EARLYCONSOLE_ARM_PL.lock(), $($x,)*);
        }
    };
}

pub fn init_from_dtb_node(node: &FdtNode) {
    for property in node.properties() {
        if property.name() == "reg" {
            let phys_addr = property.read_u64(0).unwrap() | 0xffffff8000000000;

            EARLYCONSOLE_ARM_PL.lock().ptr = Some(NonNull::new(phys_addr as _).unwrap());
        }
    }
    early_printk!("Early console initialized.\n");
}

pub fn disable_earlyconsole() {
    EARLYCONSOLE_ARM_PL.lock().ptr = None;
}
