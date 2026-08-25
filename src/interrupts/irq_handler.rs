use core::sync::atomic::{AtomicBool, Ordering::SeqCst};

use crate::{
    Gic, cpu_local, per_cpu_lock,
    sched::SCHEDULER,
    utils::{ArcAny, CpuLocal, PerCpuLock, SpinLock},
};

pub struct ReturnExceptionRegs {
    regs: ExceptionRegisters,
    to_use: bool,
}

impl ReturnExceptionRegs {
    pub const fn new() -> Self {
        Self {
            regs: unsafe { core::mem::zeroed() },
            to_use: false,
        }
    }

    pub fn put(&mut self, regs: &ExceptionRegisters) {
        self.regs = regs.clone();
        self.to_use = true;
    }

    pub fn take(&mut self) -> Option<*const ExceptionRegisters> {
        if self.to_use {
            let ptr = &raw const self.regs;
            self.to_use = false;

            Some(ptr)
        } else {
            None
        }
    }
}

pub static RETURN_TABLE: PerCpuLock<ReturnExceptionRegs> =
    per_cpu_lock!(ReturnExceptionRegs::new());

pub static IRQ_TABLE: SpinLock<IrqTable> = SpinLock::new(IrqTable {
    irqs: [const { None }; 1024],
});

static IRQ_BOOL: CpuLocal<AtomicBool> = cpu_local!(AtomicBool::new(false));

pub fn can_block() -> bool {
    !IRQ_BOOL.local().load(SeqCst)
}

pub struct IrqContext<'a> {
    data: Option<&'a ArcAny>,
}

struct IrqHandler {
    callback: fn(Option<&ArcAny>),
    data: Option<ArcAny>,
}

pub struct IrqTable {
    irqs: [Option<IrqHandler>; 1024],
}

impl IrqTable {
    pub fn register_interrupt(
        &mut self,
        irqn: u64,
        callback: fn(Option<&ArcAny>),
        data: Option<ArcAny>,
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
    pub sp_el0: u64,
    pub _pad: u64,
}

#[unsafe(no_mangle)]
extern "C" fn irq_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let irq = Gic::acknowledge();
    IRQ_BOOL.local().store(true, SeqCst);

    SCHEDULER
        .get()
        .unwrap()
        .save_register_state_to_task(unsafe { regs.as_ref().unwrap() });

    if let Some(handler) = IRQ_TABLE.lock().irqs[irq as usize].as_ref() {
        (handler.callback)(handler.data.as_ref());
    }

    Gic::complete(irq);

    let return_regs = RETURN_TABLE.lock().take().unwrap_or(regs);
    IRQ_BOOL.local().store(false, SeqCst);

    // Return to who we came from.
    return_regs
}
