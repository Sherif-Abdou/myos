use crate::{driver::Driver, early_printk, utils::MMIO};

pub struct Pl {
    regs: MMIO,
}

impl Driver for Pl {
    fn new(node: &crate::dtb::FdtNode) -> Self
    where
        Self: Sized,
    {
        let addr = node.read_u64("reg", 0).unwrap();
        let size = node.read_u64("reg", 0).unwrap();

        let mmio = MMIO::new(addr as usize, size as usize);

        Self { regs: mmio }
    }

    fn compatible_string() -> &'static str
    where
        Self: Sized,
    {
        "arm,pl011"
    }
}
