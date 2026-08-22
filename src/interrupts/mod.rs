mod gic;
mod irq_handler;
mod syscalls;

use core::arch::asm;

pub use gic::Gic;
pub use irq_handler::{ExceptionRegisters, IRQ_TABLE, RETURN_TABLE, can_block};

use crate::write_sysreg;

unsafe extern "C" {
    pub unsafe static mut __exc_vector: u8;
}

/// Unmask interrupts.
pub fn daifclr() {
    unsafe {
        asm!("msr daifclr, #0b1111");
        asm!("isb");
    }
}

/// Unmask interrupts.
pub fn daifset() {
    unsafe {
        asm!("msr daifset, #0b1111");
        asm!("isb");
    }
}

pub fn configure_exceptions() {
    // Set up the vector table.
    unsafe {
        write_sysreg!(VBAR_EL1, (&raw const __exc_vector).addr());
        asm!("isb");
    }
}
