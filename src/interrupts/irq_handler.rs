use crate::{Gic, early_printk, sched::SCHEDULER, timer::ArmTimer};

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct ExceptionRegisters {
    pub gprs: [u64; 32],
    pub elr: u64,
    pub spsr: u64,
}

#[unsafe(no_mangle)]
extern "C" fn sexc_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    regs
}

#[unsafe(no_mangle)]
extern "C" fn irq_handler(mut regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let irq = Gic::acknowledge();

    if irq == 27 {
        ArmTimer::wait(1_000_000);

        SCHEDULER.get().unwrap().save_register_state_to_task(unsafe { regs.as_ref().unwrap() });

        if let Some(new_ret) = SCHEDULER.get().unwrap().next_task() {
            Gic::complete(irq);
            return new_ret;
        }
    }

    Gic::complete(irq);

    // Return to who we came from.
    regs
}
