#include "page_alloc.hpp"
#include "mem.hpp"
#include <cstddef>
#include <cstdint>

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
    if (esr.ec == 0b100001 || esr.ec == 0b100101) {
        should_abort |= handle_kernel_mmu();
    }

    while (should_abort) {
    }
}
