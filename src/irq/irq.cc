#include "irq/gic.hpp"
#include <irq/irq.hpp>

std::array<IsrManager::Isr, 1024> IsrManager::isrs {};

void IsrManager::register_isr(int irqn, void (*isr)(void *), void *data) {
    // SGI/PPI
    if (irqn < 32) {
        Gic::enable_private_irq(irqn);
    } else { // SPI

    }
    IsrManager::isrs[irqn] = {isr, data};
}

void IsrManager::free_isr(int irqn) {
    IsrManager::isrs[irqn] = {nullptr, nullptr};
}

void IsrManager::dispatch(int irqn) {
    if (isrs[irqn].isr != nullptr) {
        isrs[irqn].isr(isrs[irqn].data);
    }
}
