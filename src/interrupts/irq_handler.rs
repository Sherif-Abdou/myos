use core::{
    any::Any,
    arch::asm,
    sync::atomic::{AtomicBool, Ordering::SeqCst},
};

use crate::{
    Gic, early_printk, printk, read_sysreg,
    sched::SCHEDULER,
    utils::{Arc, PerCpuLock, SpinLock},
};

pub static RETURN_TABLE: PerCpuLock<Option<*const ExceptionRegisters>> = PerCpuLock::nones();

pub static IRQ_TABLE: SpinLock<IrqTable> = SpinLock::new(IrqTable {
    irqs: [const { None }; 1024],
});

static IRQ_BOOL: AtomicBool = AtomicBool::new(false);

pub fn can_block() -> bool {
    IRQ_BOOL.load(SeqCst)
}

pub struct IrqContext<'a> {
    data: Option<&'a Arc<dyn Any + Sync + Send>>,
}

struct IrqHandler {
    callback: fn(Option<&Arc<dyn Any + Sync + Send>>),
    data: Option<Arc<dyn Any + Sync + Send>>,
}

pub struct IrqTable {
    irqs: [Option<IrqHandler>; 1024],
}

impl IrqTable {
    pub fn register_interrupt(
        &mut self,
        irqn: u64,
        callback: fn(Option<&Arc<dyn Any + Send + Sync>>),
        data: Option<Arc<dyn Any + Send + Sync>>,
    ) {
        self.irqs[irqn as usize] = Some(IrqHandler { callback, data });
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct ExceptionRegisters {
    pub gprs: [u64; 32],
    pub elr: u64,
    pub spsr: u64,
}

#[unsafe(no_mangle)]
extern "C" fn sexc_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    early_printk!("EXCEPTION: \n");
    let far: u64;
    unsafe {
        read_sysreg!(far, FAR_EL1);
    }
    let elr: u64;
    unsafe {
        read_sysreg!(elr, ELR_EL1);
    }

    printk!("EXCEPTION at 0x{:x}, FAR 0x{:x} \n", elr, far);
    unsafe {
        printk!("x0: {:x}\n", (*regs).gprs[0]);
        printk!("x1: {:x}\n", (*regs).gprs[1]);
        printk!("x2: {:x}\n", (*regs).gprs[2]);
        printk!("x3: {:x}\n", (*regs).gprs[3]);
        printk!("x4: {:x}\n", (*regs).gprs[4]);
        printk!("x5: {:x}\n", (*regs).gprs[5]);
        printk!("x6: {:x}\n", (*regs).gprs[6]);
        printk!("x7: {:x}\n", (*regs).gprs[7]);
        printk!("x8: {:x}\n", (*regs).gprs[8]);
        printk!("x9: {:x}\n", (*regs).gprs[9]);
        printk!("x10: {:x}\n", (*regs).gprs[10]);
        printk!("x11: {:x}\n", (*regs).gprs[11]);
        printk!("x12: {:x}\n", (*regs).gprs[12]);
        printk!("x13: {:x}\n", (*regs).gprs[13]);
        printk!("x14: {:x}\n", (*regs).gprs[14]);
        printk!("x15: {:x}\n", (*regs).gprs[15]);
        printk!("x16: {:x}\n", (*regs).gprs[16]);
        printk!("x17: {:x}\n", (*regs).gprs[17]);
        printk!("x18: {:x}\n", (*regs).gprs[18]);
        printk!("x19: {:x}\n", (*regs).gprs[19]);
        printk!("x20: {:x}\n", (*regs).gprs[20]);
        printk!("x21: {:x}\n", (*regs).gprs[21]);
        printk!("x22: {:x}\n", (*regs).gprs[22]);
        printk!("x23: {:x}\n", (*regs).gprs[23]);
        printk!("x24: {:x}\n", (*regs).gprs[24]);
        printk!("x25: {:x}\n", (*regs).gprs[25]);
        printk!("x26: {:x}\n", (*regs).gprs[26]);
        printk!("x27: {:x}\n", (*regs).gprs[27]);
        printk!("x28: {:x}\n", (*regs).gprs[28]);
        printk!("x29: {:x}\n", (*regs).gprs[29]);
        printk!("x30: {:x}\n", (*regs).gprs[30]);
        printk!("sp: {:x}\n", (*regs).gprs[31]);
        printk!("sp: {:x}\n", (*regs).gprs[31]);
    }

    loop {
        unsafe {
            asm!("wfi");
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn irq_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let irq = Gic::acknowledge();
    IRQ_BOOL.store(true, SeqCst);

    SCHEDULER
        .get()
        .unwrap()
        .save_register_state_to_task(unsafe { regs.as_ref().unwrap() });

    if let Some(handler) = IRQ_TABLE.lock().irqs[irq as usize].as_ref() {
        (handler.callback)(handler.data.as_ref());
    }

    Gic::complete(irq);

    let return_regs = RETURN_TABLE.lock().take().unwrap_or(regs);
    IRQ_BOOL.store(false, SeqCst);

    // Return to who we came from.
    return_regs
}
