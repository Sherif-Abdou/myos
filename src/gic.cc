#include "gic.hpp"
#include "utils.hpp"
#include <cstdint>

void Gic::enable_system_registers() {
    uint64_t sre = 1;
    write_sysreg(ICC_SRE_EL1, sre);
}

void Gic::init_distributor() {
}

void Gic::init() {
    enable_system_registers();
}
