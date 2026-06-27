#include "irq/irq.hpp"
#include "mem.hpp"
#include "print/printk.hpp"
#include "utils.hpp"
#include <cstddef>
#include <cstdint>
#include <irq/gic.hpp>

struct esr_t {
    union {
        struct {
            uint64_t iss : 25;
            uint64_t il : 1;
            uint64_t ec : 6;
            uint64_t iss2 : 24;
            uint64_t : 8;
        };
        uint64_t raw;
    };
};

bool handle_kernel_mmu(void) {
    bool should_abort = true;
    size_t fault_address;

    asm volatile("mrs %0, far_el1" : "=r"(fault_address));

    return should_abort;
}

extern "C" void sexc_handler(void) {
    bool should_abort = true;
    esr_t esr;

    asm volatile("mrs %0, esr_el1" : "=r"(esr.raw));

    // Kernel instruction or data mmu fault
    if (esr.ec == 0b000011) {
        early_printk("Error: Unhandled mcr/mrc fault, aborting.\n");
    } else if (esr.ec == 0b010101) {
        early_printk("Error: Unhandled svc trap, aborting.\n");
    } else if (esr.ec == 0b100001 || esr.ec == 0b100101) {
        should_abort |= handle_kernel_mmu();
        if (should_abort) {
            early_printk("Error: Unhandled page fault, aborting.\n");
        }
    } else {
        early_printk("Error: Unhandled exception, 0x", Hex(esr.ec) ," aborting.\n");
    }
    uint64_t elr;
    read_sysreg(elr, ELR_EL1);
    early_printk("Source address: 0x", Hex(elr) ,"\n");

    while (should_abort) {
        asm volatile("wfi");
    }
}

extern "C" void irq_handler(void) {
    uint32_t irq = Gic::acknowledge();
    IsrManager::dispatch(irq);
    Gic::complete(irq);
}
