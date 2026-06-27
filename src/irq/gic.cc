#include "gic.hpp"
#include "byte_alloc.hpp"
#include "utils.hpp"
#include <cstdint>

std::optional<Gic*> Gic::gic = std::nullopt;

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

void Gic::create_gic(VolatileRegion distributor, VolatileRegion redistributor) {
    gic = {ByteAllocator::kalloc<Gic>(distributor, redistributor)};
    (*gic)->init();
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

void Gic::enable_private_irq(uint64_t irqn) {
    Gic & gic = *Gic::gic.value();
    gic.redistributor.writew(~bit(1U), GICR_WAKER);
    while ((gic.redistributor.readw(GICR_WAKER) & bit(2ul)) != 0);

    gic.redistributor.writew(0x80, 0x10000 + GICD_IPRIORITY(irqn));
    uint32_t enable = gic.redistributor.readw(0x10000 + GICR_ISENABLER0);
    enable |= bit(irqn);
    gic.redistributor.writew(enable, 0x10000 + GICR_ISENABLER0);

    uint32_t group = gic.redistributor.readw(0x10000 + GICR_IGROUPR0);
    gic.redistributor.writew(group | bit(irqn % 32), 0x10000 + GICR_IGROUPR0);

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
