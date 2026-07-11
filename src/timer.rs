use crate::{read_sysreg, write_sysreg};

pub struct ArmTimer {}

impl ArmTimer {
    pub fn enable() {
        let mut enable: u64 = 0;
        unsafe {
            read_sysreg!(enable, CNTV_CTL_EL0);
            enable |= 1;
            write_sysreg!(CNTV_CTL_EL0, enable);
        }
    }

    pub fn wait(micros: u64) {
        let mut freq = 0u64;
        unsafe {
            read_sysreg!(freq, CNTFRQ_EL0);

            let ticks = (micros * freq) / 1_000_000;
            write_sysreg!(CNTV_TVAL_EL0, ticks);
        }
    }
}
