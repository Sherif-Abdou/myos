use crate::{Gic, early_printk, timer::ArmTimer};

#[repr(C)]
struct ExceptionRegisters {
    gprs: [u64; 32],
    elr: u64,
    spsr: u64,
}

#[unsafe(no_mangle)]
extern "C" fn sexc_handler() {}

#[unsafe(no_mangle)]
extern "C" fn irq_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let irq = Gic::acknowledge();
    early_printk!("Interrupt.\n");

    if irq == 27 {
        ArmTimer::wait(1_000_000);
        unsafe {
            let gpr0 = (*regs).gprs[0];
            early_printk!("x0: {:x}\n", gpr0);
        }
    }

    Gic::complete(irq);

    // Return to who we came from.
    regs
}
