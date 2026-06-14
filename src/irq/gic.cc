#include "gic.hpp"
#include "utils.hpp"
#include <cstdint>

void Gic::enable_system_registers() {
    uint64_t sre = bit(0UL);
    write_sysreg(ICC_SRE_EL1, sre);
    isb();
}

void Gic::init_distributor() {
    while ((distributor.readl(GICD_CTRL) & bit(31UL)) != 0)
        ;

    distributor.writew(0, GICD_CTRL);
    uint32_t ctrl = distributor.readw(GICD_CTRL);
    // Enable affinity routing
    ctrl |= bit(4UL);
    // Enable interrupts
    ctrl |= (bit(1UL) | bit(2UL));
    distributor.writew(ctrl, GICD_CTRL);

    while ((distributor.readl(GICD_CTRL) & bit(31UL)) != 0)
        ;
}

Gic::Gic(VolatileRegion distributor, VolatileRegion redistributor)
    : distributor(distributor), redistributor(redistributor) {}

void Gic::init() {
    enable_system_registers();
    init_distributor();
}

uint64_t Gic::interrupt_available() {
    uint64_t intid;
    read_sysreg(intid, ICC_HPPIR1_EL1);
    if (intid == 1020 || intid == 1021 || intid == 1023)
        return 1023;
    return intid;
}

void Gic::enable_ppi(uint64_t irqn) {
    redistributor.writew(~bit(1U), GICR_WAKER);
    while ((redistributor.readw(GICR_WAKER) & bit(2ul)) != 0);

    redistributor.writew(0x80, 0x10000 + GICD_IPRIORITY(irqn));
    uint32_t enable = redistributor.readw(0x10000 + GICR_ISENABLER0);
    enable |= bit(irqn);
    redistributor.writew(enable, 0x10000 + GICR_ISENABLER0);

    uint32_t group = redistributor.readw(0x10000 + GICR_IGROUPR0);
    redistributor.writew(group | bit(irqn % 32), 0x10000 + GICR_IGROUPR0);

}

void Gic::set_cpu_prio(uint64_t prio) {
    write_sysreg(ICC_PMR_EL1, prio);
}

void Gic::enable_irqs() {
    write_sysreg(ICC_IGRPEN1_EL1, 1UL);
}

void Gic::acknowledge(uint64_t irq) {
    write_sysreg(ICC_IAR1_EL1, irq);
}

uint32_t Gic::acknowledge() {
    uint64_t irq;
    read_sysreg(irq, ICC_IAR1_EL1);
    return irq;
}

void Gic::complete(uint64_t irq) {
    write_sysreg(ICC_EOIR1_EL1, irq);
}
