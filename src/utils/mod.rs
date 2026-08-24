mod addr;
mod arc;
mod core_lock;
mod cpu_local;
mod deque;
mod intrusive_list;
mod mmio;
mod spin_lock;
mod string;

pub use addr::PhysAddr;
pub use arc::*;
pub use core_lock::*;
pub use cpu_local::*;
pub use deque::Deque;
pub use intrusive_list::*;
pub use mmio::Mmio;
pub use spin_lock::*;
pub use string::*;

pub const MAX_CPUS: usize = 8;

#[macro_export]
macro_rules! write_sysreg {
    ($reg:tt,$var:expr) => {
        ::core::arch::asm!(concat!("msr ",stringify!($reg),", {0:x}"), in(reg) $var);
    };
}

#[macro_export]
macro_rules! read_sysreg {
    ($var:expr,$reg:tt) => {
        ::core::arch::asm!(concat!("mrs {0:x},", stringify!($reg)), out(reg) $var);
    };
}

pub fn cpu_id() -> usize {
    let mut id = 0usize;

    unsafe {
        read_sysreg!(id, MPIDR_EL1);
    }

    id & 0xff
}
