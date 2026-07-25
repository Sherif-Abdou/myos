use core::any::Any;

use crate::{
    GIC, arm_pl::disable_earlyconsole, driver::Driver, dtb::FdtNode, interrupts::{IRQ_TABLE, can_block}, sched::WaitQueue, subsystem::{ConsoleDriver, set_console}, utils::{Arc, Mmio},
};

const RING_BUFFER_SIZE: usize = 1024;

const UARTDR: usize = 0x0;
const UARTRCR: usize = 0x4;
const UARTFR: usize = 0x18;
const UARTILPR: usize = 0x20;
const UARTIBRD: usize = 0x24;
const UARTLCR_H: usize = 0x28;
const UARTCR: usize = 0x30;
const UARTIFLS: usize = 0x34;
const UARTIMSC: usize = 0x38;
const UARTRIS: usize = 0x3C;
const UARTMIS: usize = 0x40;
const UARTICR: usize = 0x44;
const UARTDMACR: usize = 0x44;

fn irq_handler(driver: Option<&Arc<dyn Any + Send + Sync + 'static>>) {
    let driver: &Pl = driver
        .map(|driver| driver.downcast_ref::<Pl>().unwrap())
        .expect("Driver must be passed in");

    let status = unsafe { driver.regs.read_u16(UARTMIS) };
    if status & (1 << 5) != 0 {
        if driver.wait_queue.is_empty() {
            driver.disable_tx_irq();
        } else {
            driver.wait_queue.unblock_front();
        }
    }
    if status & (1 << 4) != 0 {
        while unsafe { driver.regs.read_u16(UARTFR) } & (1 << 4) == 0 {
            let _ = unsafe { driver.regs.read_u8(UARTDR) };
        }
    }

    unsafe {
        driver.regs.write_u16(status, UARTICR);
    }
}

pub struct Pl {
    regs: Mmio,
    wait_queue: WaitQueue,
}

impl Pl {
    fn setup_fifo(&self) {
        // Enable fifo
        unsafe {
            self.regs.modify_u8(UARTCR, |before| before & !(1 << 0));
            self.regs.modify_u8(UARTLCR_H, |before| before & !(1 << 4));
            self.regs.modify_u8(UARTLCR_H, |before| before | (1 << 4));
            self.regs.modify_u8(UARTCR, |before| before | (1 << 0));
        }
    }

    fn enable_tx_irq(&self) {
        unsafe {
            self.regs.modify_u8(UARTIMSC, |before| before | (1 << 5));
        }
    }

    fn enable_rx_irq(&self) {
        unsafe {
            self.regs.modify_u8(UARTIMSC, |before| before | (1 << 4));
        }
    }

    fn disable_tx_irq(&self) {
        unsafe {
            self.regs.modify_u8(UARTIMSC, |before| before & !(1 << 5));
        }
    }
}

impl ConsoleDriver for Pl {
    fn write_bytes(&self, buf: &[u8]) {
        for c in buf {
            while unsafe { self.regs.read_u16(UARTFR) } & (1 << 5) != 0 {
                if can_block() {
                    self.wait_queue.enqueue_and_block();
                    self.enable_tx_irq();
                } else {
                    core::hint::spin_loop();
                }
            }

            unsafe {
                self.regs.write_u8(*c, UARTDR);
            }
        }
    }
}

impl Driver for Pl {
    fn new(node: &FdtNode) -> Result<Self, ()>
    where
        Self: Sized,
    {
        let addr = node.read_u64("reg", 0).unwrap();
        let size = node.read_u64("reg", 1).unwrap();

        let mmio = Mmio::new(addr as usize, size as usize);

        Ok(Self {
            regs: mmio,
            wait_queue: WaitQueue::new(),
        })
    }

    fn init(driver: Arc<Pl>, _node: &FdtNode) {
        driver.setup_fifo();

        IRQ_TABLE
            .lock()
            .register_interrupt(33, irq_handler, Some(driver.clone()));
        GIC.get().unwrap().enable_spi(33);

        driver.enable_rx_irq();

        disable_earlyconsole();
        set_console(driver);
    }

    fn compatible_string() -> &'static str
    where
        Self: Sized,
    {
        "arm,pl011"
    }
}
