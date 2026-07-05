mod core_lock;
mod intrusive_list;
mod spin_lock;

use core::arch::asm;

pub use core_lock::*;
pub use intrusive_list::*;
pub use spin_lock::*;

pub const MAX_CPUS: usize = 8;

pub fn cpu_id() -> usize {
    let mut id = 0usize;

    unsafe {
        asm!("mrs {0}, MPIDR_EL1", out(reg) id);
    }

    id & 0xff
}
