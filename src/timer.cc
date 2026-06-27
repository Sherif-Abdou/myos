#include "timer.hpp"

#include <cstdint>
#include <utils.hpp>

ArmTimer ArmTimer::timer {};

void ArmTimer::init() {
    read_sysreg(counter_frequency, CNTFRQ_EL0);
}

void ArmTimer::enable() {
    uint64_t enable;
    read_sysreg(enable, CNTV_CTL_EL0);
    enable |= bit(0ul);
    write_sysreg(CNTV_CTL_EL0, enable);
}

void ArmTimer::disable() {
    uint64_t disable;
    read_sysreg(disable, CNTV_CTL_EL0);
    disable ^= ~bit(0ul);
    write_sysreg(CNTV_CTL_EL0, disable);
}

void ArmTimer::set_frequency(uint64_t frequency) {
    uint64_t delta = ((counter_frequency + frequency - 1) / frequency) & 0xffffffff;

    write_sysreg(CNTV_TVAL_EL0, delta);
}

void ArmTimer::set_delay(uint64_t microseconds) {
    uint64_t target_frequency = (microseconds * 1'000'000);
    uint64_t delta = ((counter_frequency + target_frequency - 1) / target_frequency) & 0xffffffff;

    write_sysreg(CNTV_TVAL_EL0, delta);
}
