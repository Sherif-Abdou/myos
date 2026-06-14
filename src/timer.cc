#include "timer.hpp"

#include <cstdint>
#include <utils.hpp>

ArmTimer ArmTimer::timer {};

void ArmTimer::init() {
    read_sysreg(counter_frequency, CNTFRQ_EL0);
}

void ArmTimer::enable() {
    uint64_t enable = bit(0ul);
    write_sysreg(CNTV_CTL_EL0, enable);
}

void ArmTimer::set_frequency(uint64_t frequency) {
    uint64_t delta = ((counter_frequency + frequency - 1) / frequency) & 0xffffffff;

    write_sysreg(CNTV_TVAL_EL0, delta);
}
