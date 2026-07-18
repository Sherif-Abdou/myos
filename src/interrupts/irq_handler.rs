use core::{any::Any, arch::asm, sync::atomic::{AtomicBool, Ordering::SeqCst}};

use crate::{
    Gic, early_printk,
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
    loop {
        unsafe {
            asm!("wfi");
        }
    }
    regs
}

#[unsafe(no_mangle)]
extern "C" fn irq_handler(mut regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
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
