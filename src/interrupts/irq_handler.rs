use core::{
    arch::asm,
    ffi::CStr,
    sync::atomic::{AtomicBool, Ordering::SeqCst},
};

use crate::{
    Gic, early_printk, printk, read_sysreg,
    sched::SCHEDULER,
    utils::{ArcAny, PerCpuLock, SpinLock},
    write_sysreg,
};

pub static RETURN_TABLE: PerCpuLock<Option<*const ExceptionRegisters>> = PerCpuLock::nones();

pub static IRQ_TABLE: SpinLock<IrqTable> = SpinLock::new(IrqTable {
    irqs: [const { None }; 1024],
});

static IRQ_BOOL: AtomicBool = AtomicBool::new(false);

pub fn can_block() -> bool {
    !IRQ_BOOL.load(SeqCst)
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
    let esr: u64;
    unsafe {
        read_sysreg!(esr, ESR_EL1);
    }

    let ec = (esr >> 26) & 0x3f;

    if ec == 0x15 {
        let call = esr & 0xff;

        if call == 27 {
            let addr = unsafe { (*regs).gprs[0] } as *mut u8;

            let cstr = unsafe { CStr::from_ptr(addr) };

            let str = cstr.to_str().unwrap();

            printk!("{}", str);
            regs
        } else if call == 50 {
            SCHEDULER.get().unwrap().end_task();

            let next_task = SCHEDULER.get().unwrap().next_task().unwrap();

            next_task
        } else {
            regs
        }
    } else {
        printk!(
            "EXCEPTION at 0x{:x}, FAR 0x{:x}, ESR 0x{:x} \n",
            elr,
            far,
            esr
        );
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
        }

        match ec {
            0x1 => {
                printk!("Trapped WF* Instruction\n");
            }
            0x3 => {
                printk!("Trapped MCR or MRC\n");
            }
            0x7 => {
                printk!("Trapped FPU\n");
            }
            0xd => {
                printk!("Branch target exception\n");
            }
            0xe => {
                printk!("Illegal execution state\n");
            }
            0x15 => {
                printk!("Trapped SVC\n");
            }
            _ => {
                printk!("EC: {:x}\n", ec);
            }
        }

        loop {
            unsafe {
                asm!("wfi");
            }
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn irq_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    // unsafe {
    //     asm!("msr PAN, #0");
    // }
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
