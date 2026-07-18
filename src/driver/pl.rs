use core::any::Any;

use crate::{
    GIC,
    driver::Driver,
    dtb::FdtNode,
    interrupts::IRQ_TABLE,
    utils::{Arc, Deque, MMIO, SpinLock},
};

const RING_BUFFER_SIZE: usize = 1024;

fn irq_handler(driver: Option<&Arc<dyn Any + Send + Sync + 'static>>) {
    let driver: &Arc<Pl> = driver.map(|driver| driver.downcast_ref().unwrap()).unwrap();
}

pub struct Pl {
    regs: MMIO,
    deque: SpinLock<Deque<u8, RING_BUFFER_SIZE>>,
}

impl Driver for Pl {
    fn new(node: &FdtNode) -> Self
    where
        Self: Sized,
    {
        let addr = node.read_u64("reg", 0).unwrap();
        let size = node.read_u64("reg", 1).unwrap();

        let mmio = MMIO::new(addr as usize, size as usize);

        Self {
            regs: mmio,
            deque: SpinLock::new(Deque::new()),
        }
    }

    fn init(driver: Arc<Pl>, node: &FdtNode) {
        IRQ_TABLE.lock().register_interrupt(33, irq_handler, Some(driver.clone()));
        GIC.get().unwrap().enable_spi(33);

        unsafe {
            driver.regs.modify_u32(0x38, |before| before | (1 << 4));
        }
    }

    fn compatible_string() -> &'static str
    where
        Self: Sized,
    {
        "arm,pl011"
    }
}
