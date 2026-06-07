#include <cstdint>
#include <page.hpp>
#include <page_alloc.hpp>

extern "C" void abort();

constinit static PageManager<4> ttb0_manager{};
constinit static PageManager<4> ttb1_manager{};

constinit static PageAllocator page_allocator{};

#define SYMBOL_ADDRESS(x) (virt_to_page((size_t)x) << 12)

extern "C" {
extern volatile char __boot_start[];
extern volatile char __boot_end[];
extern volatile char __text_start[];
extern volatile char __text_end[];
extern volatile char __data_start[];
extern volatile char __data_end[];
extern volatile char __bss_start[];
extern volatile char __bss_end[];
extern volatile char __stack_start[];
extern volatile char __stack_end[];

extern volatile char __exc_vector[];
}

void build_kernel_pt() {
    page_allocator.reserve_local_pages();
    page_allocator.alloc_from_start_end(SYMBOL_ADDRESS(__boot_start),
                                        SYMBOL_ADDRESS(__boot_end));
    page_allocator.alloc_from_start_end(SYMBOL_ADDRESS(__text_start),
                                        SYMBOL_ADDRESS(__text_end));
    page_allocator.alloc_from_start_end(SYMBOL_ADDRESS(__data_start),
                                        SYMBOL_ADDRESS(__data_end));
    page_allocator.alloc_from_start_end(SYMBOL_ADDRESS(__bss_start),
                                        SYMBOL_ADDRESS(__bss_end));
    page_allocator.alloc_from_start_end(SYMBOL_ADDRESS(__stack_start),
                                        SYMBOL_ADDRESS(__stack_end));
    page_allocator.alloc_from_ptr(&ttb0_manager);
    page_allocator.alloc_from_ptr(&ttb1_manager);

    auto kernel_l2_pages = page_allocator.reserve_free_range(2);
    auto l2_page_manager = kernel_l2_pages->make<PageManager<512>>();

    for (size_t index = 0; index < l2_page_manager->size(); ++index) {
        l2_page_manager->modify_page(index, [&](page_descriptor_t &descriptor) {
            descriptor.fields.nblock = 0;
            descriptor.fields.valid = 1;
            /* Deice nGnRnE memory assumed */
            descriptor.fields.mem_attrs = 1;
            descriptor.fields.af = 1;
            descriptor.fields.nlta = index << 9 | (0x40000000 >> 12);
        });
    }

    ttb1_manager.modify_page(0, [&](page_descriptor_t &descriptor) {
        descriptor.fields.valid = 1;
        descriptor.fields.nblock = 0;
        descriptor.fields.af = 1;
        descriptor.fields.nlta = 0;
    });

    ttb1_manager.modify_page(1, [&](page_descriptor_t &descriptor) {
        descriptor.fields.valid = 1;
        descriptor.fields.nblock = 1;
        descriptor.fields.af = 1;
        descriptor.fields.nlta =
            virt_to_page(std::bit_cast<size_t>(l2_page_manager));
    });

    auto ttbr_addr = virt_to_page(reinterpret_cast<size_t>(&ttb1_manager))
                     << 12;

    asm volatile(R"(
        tlbi vmalle1
        dsb ish
        isb sy
        msr ttbr1_el1, %0
        isb sy
    )" ::"r"(ttbr_addr));
}

extern "C" {
__attribute__((noinline, used)) void register_exception_handler() {
    uintptr_t exception_addr = (uintptr_t)__exc_vector;

    asm volatile("msr vbar_el1, %0\nisb\n" ::"r"(exception_addr) : "memory");
}
void loop() {
    build_kernel_pt();
    // register_exception_handler();

    for (int i = 0; i < 10; ++i) {
        asm volatile("nop");
    }

    abort();
}

void abort() {
    while (1) {
        asm volatile("wfe");
    }
}

void _exc_entry() { asm volatile("b _exc_handler"); }

void _exc_handler() {
    // save context
    asm volatile(R"(
        stp x0, x1, [sp, #-16]!
        stp x2, x3, [sp, #-16]!
        stp x4, x5, [sp, #-16]!
        stp x6, x7, [sp, #-16]!
        stp x8, x9, [sp, #-16]!
        stp x10, x11, [sp, #-16]!
        stp x12, x13, [sp, #-16]!
        stp x14, x15, [sp, #-16]!
        stp x16, x17, [sp, #-16]!
        stp x18, x19, [sp, #-16]!
        stp x20, x21, [sp, #-16]!
        stp x22, x23, [sp, #-16]!
        stp x24, x25, [sp, #-16]!
        stp x26, x27, [sp, #-16]!
        stp x28, x29, [sp, #-16]!
        str x30, [sp, #-16]!
        mrs x0, elr_el1
        mrs x1, spsr_el1
        stp x0, x1, [sp, #-16]! 
    )" ::
                     : "memory");

    while (1) {
    }

    // restore context
    asm volatile(R"(
        ldp x0, x1, [sp], #16 
        msr x0, elr_el1
        msr x1, spsr_el1
        ldr x30, [sp], #16
        ldp x28, x29, [sp], #16
        ldp x26, x27, [sp], #16
        ldp x24, x25, [sp], #16
        ldp x22, x23, [sp], #16
        ldp x20, x21, [sp], #16
        ldp x18, x19, [sp], #16
        ldp x16, x17, [sp], #16
        ldp x14, x15, [sp], #16
        ldp x12, x13, [sp], #16
        ldp x10, x11, [sp], #16
        ldp x8, x9, [sp], #16
        ldp x6, x7, [sp], #16
        ldp x4, x5, [sp], #16
        ldp x2, x3, [sp], #16
        ldp x0, x1, [sp], #16
        eret
    )" ::
                     : "memory");
}

asm(R"(
)");
}
