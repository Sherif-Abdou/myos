use crate::{
    dtb::FdtNode,
    read_sysreg,
    utils::{Mmio, PerCpuLock, SpinLock},
    write_sysreg,
};

// Distributor control register.
const GICD_CTRL: usize = 0;
// Interrupt controller type register.
const GICD_TYPER: usize = 0x4;
// Distributor Implementer Identification Register.
const GICD_IIDR: usize = 0x8;
// Interrupt Controller Type 2 Register.
const GICD_TYPER2: usize = 0xC;
// Error Reporting Status Register (optional).
const GICD_STATUSR: usize = 0x10;
// Set SPI Register.
const GICD_SETSPI_NSR: usize = 0x40;
// Clear SPI Register.
const GICD_CLRSPI_NSR: usize = 0x48;
// Interrupt Group Registers.
const fn gicd_igroupr(n: usize) -> usize {
    0x080 + 0x4 * n
}
// Interrupt Set-Enable Registers.
const fn gicd_isenabler(n: usize) -> usize {
    0x100 + 0x4 * n
}
// Interrupt Clear-Enable Registers.
const fn gicd_icenabler(n: usize) -> usize {
    0x180 + 0x4 * n
}
// Interrupt Set-Pending Registers.
const fn gicd_ispendr(n: usize) -> usize {
    0x200 + 0x4 * n
}
// Interrupt Clear-Pending Registers.
const fn gicd_icpendr(n: usize) -> usize {
    0x280 + 0x4 * n
}
// Interrupt Set-Active Registers.
const fn gicd_isactiver(n: usize) -> usize {
    0x300 + 0x4 * n
}
// Interrupt Clear-Active Registers.
const fn gicd_icactiver(n: usize) -> usize {
    0x380 + 0x4 * n
}
// Interrupt Priority Registers.
const fn gicd_ipriority(n: usize) -> usize {
    0x400 + 0x4 * n
}
// Interrupt Processor Target Registers.
const fn gicd_itargetsr(n: usize) -> usize {
    0x800 + 0x4 * n
}
// Interrupt Configuration Registers.
const fn gicd_icfgr(n: usize) -> usize {
    0xC00 + 0x4 * n
}
// Non-secure Access Registers.
const fn gicd_nsacr(n: usize) -> usize {
    0xE00 + 0x4 * n
}
// Software Generation Interrupt Registers.
const GICD_SGIR: usize = 0xF00;
// Interrupt Routing Registers.
const fn gicd_irouter(n: usize) -> usize {
    0x6100 + 0x4 * n
}

/* Redistributor Registers. */
/// Redistributor Control Register.
const GICR_CTLR: usize = 0x0;
/// Implementer Identification Register.
const GICR_IIDR: usize = 0x4;
/// Redistributor Type Register.
const GICR_TYPER: usize = 0x8;
/// Error Reporting Status Register, optional.
const GICR_STATUSR: usize = 0x10;
/// Redistributor Wake Register.
const GICR_WAKER: usize = 0x14;
/// Report maximum PARTID and PMG Register.
const GICR_MPAMIDR: usize = 0x18;
/// Set PARTID and PMG Register.
const GICR_PARTIDR: usize = 0x1C;
/// Redistributor Properties Base Address Register.
const GICR_PROPBASER: usize = 0x70;
/// Redistributor LPI Pending Tabler Base Address Register.
const GICR_PENDBASER: usize = 0x78;

/// Interrupt Group Register 0.
const GICR_IGROUPR0: usize = 0x80;
/// Interrupt Set-Enable Register 0.
const GICR_ISENABLER0: usize = 0x100;
/// Interrupt Clear-Enable Register 0.
const GICR_ICENABLER0: usize = 0x180;
/// Interrupt Set-Pending Register 0.
const GICR_ISPENDR0: usize = 0x200;
/// Interrupt Clear-Pending Register 0.
const GICR_ICPENDR0: usize = 0x280;
/// Interrupt Set-Active Register 0.
const GICR_ISACTIVER0: usize = 0x200;
/// Interrupt Clear-Active Register 0.
const GICR_ICACTIVER0: usize = 0x280;

pub struct Gic {
    distributor: SpinLock<GicDistributor>,
    redistributors: PerCpuLock<GicRedistributor>,
}

impl Gic {
    pub fn from_node(node: &FdtNode) -> Self {
        let distributor_base = node.read_u64("reg", 0).unwrap();
        let distributor_size = node.read_u64("reg", 1).unwrap();

        let distributor = SpinLock::new(GicDistributor {
            distributor_base: Mmio::new(distributor_base as usize, distributor_size as usize),
        });

        distributor.lock().init();

        let redistributor_base = node.read_u64("reg", 2).unwrap();
        let redistributor_size = node.read_u64("reg", 3).unwrap();

        let redistributors = PerCpuLock::from_fn(|cpu_id| {
            let mmio = Mmio::new(
                redistributor_base as usize + 0x20000 * cpu_id,
                redistributor_size as usize,
            );

            GicRedistributor {
                redistributor_base: mmio,
            }
        });

        Self {
            distributor,
            redistributors,
        }
    }

    pub fn enable_local_ppi(&self, irqn: usize) {
        self.redistributors.lock().enable_ppi(irqn);
    }

    pub fn disable_local_ppi(&self, irqn: usize) {
        self.redistributors.lock().disable_ppi(irqn);
    }

    pub fn enable_spi(&self, irqn: usize) {
        self.distributor.lock().enable_spi(irqn);
    }

    pub fn configure_spi(&self, irqn: usize, config: u32) {
        self.distributor.lock().configure_spi(irqn, config);
    }

    pub fn disable_spi(&self, irqn: usize) {
        self.distributor.lock().disable_spi(irqn);
    }

    /// Initialize core specific GIC cpu interface, PPIs, and SGIs
    pub fn local_init(&self) {
        unsafe {
            write_sysreg!(ICC_SRE_EL1, 1);
        }
    }

    pub fn acknowledge() -> u64 {
        unsafe {
            let mut irqn: u64 = 0;
            read_sysreg!(irqn, ICC_IAR1_EL1);
            irqn
        }
    }

    pub fn complete(irqn: u64) {
        unsafe {
            write_sysreg!(ICC_EOIR1_EL1, irqn);
        }
    }

    pub fn set_local_priority(prio: u8) {
        unsafe {
            write_sysreg!(ICC_PMR_EL1, prio);
        }
    }

    pub fn enable_local_interrupts() {
        unsafe {
            write_sysreg!(ICC_IGRPEN1_EL1, 1);
        }
    }
}

struct GicDistributor {
    distributor_base: Mmio,
}

struct GicRedistributor {
    redistributor_base: Mmio,
}

impl GicRedistributor {
    fn wakeup(&self) {
        unsafe {
            self.redistributor_base.write_u32(!(1 << 1), GICR_WAKER);
            while self.redistributor_base.read_u32(GICR_WAKER) & (1 << 2) != 0 {
                core::hint::spin_loop();
            }
        };
    }

    pub fn enable_ppi(&self, irqn: usize) {
        assert!(irqn < 32);
        self.wakeup();
        unsafe {
            self.redistributor_base
                .write_u32(1 << irqn as u32, 0x10000 + GICR_ISENABLER0);

            self.redistributor_base
                .modify_u32(0x10000 + GICR_IGROUPR0, |reg| reg | (1 << irqn));
        }
    }

    pub fn disable_ppi(&self, irqn: usize) {
        assert!(irqn < 32);
        self.wakeup();
        unsafe {
            self.redistributor_base
                .write_u32(1 << irqn as u32, 0x10000 + GICR_ICENABLER0);
        }
    }
}

impl GicDistributor {
    pub fn init(&self) {
        while unsafe { self.distributor_base.read_u32(GICD_CTRL) & (1 << 31) } != 0 {
            core::hint::spin_loop();
        }

        // Enable interrupts and affinity routing.
        unsafe {
            self.distributor_base
                .write_u32((1 << 1) | (1 << 2) | (1 << 4), GICD_CTRL);
        }

        while unsafe { self.distributor_base.read_u32(GICD_CTRL) & (1 << 31) } != 0 {
            core::hint::spin_loop();
        }
    }

    pub fn enable_spi(&self, irqn: usize) {
        // while unsafe { self.distributor_base.read_u32(GICD_CTRL) & (1 << 31) } != 0 {
        //     core::hint::spin_loop();
        // }

        let index = (irqn) / 32;
        let offset = (irqn) % 32;
        unsafe {
            self.distributor_base
                .modify_u32(gicd_isenabler(index), |before| before | (1 << offset));
            self.distributor_base
                .modify_u32(gicd_igroupr(index), |before| before | (1 << offset));
            self.distributor_base
                .write_u8(0x80, gicd_ipriority(0) + irqn);
        }
        // while unsafe { self.distributor_base.read_u32(GICD_CTRL) & (1 << 31) } != 0 {
        //     core::hint::spin_loop();
        // }
    }

    pub fn configure_spi(&self, irqn: usize, config: u32) {
        let index = (irqn) / 16;
        let offset = ((irqn) % 16) * 2;

        unsafe {
            self.distributor_base
                .modify_u32(gicd_icfgr(index), |before| {
                    before | (config & 0x3) << offset
                });
        }
    }

    pub fn disable_spi(&self, irqn: usize) {
        while unsafe { self.distributor_base.read_u32(GICD_CTRL) & (1 << 31) } != 0 {
            core::hint::spin_loop();
        }

        let index = irqn / 32;
        let offset = irqn % 32;
        unsafe {
            self.distributor_base
                .modify_u32(gicd_icenabler(index), |before| before | (1 << offset));
        }
        while unsafe { self.distributor_base.read_u32(GICD_CTRL) & (1 << 31) } != 0 {
            core::hint::spin_loop();
        }
    }
}
