#include "byte_alloc.hpp"
#include "irq/gic.hpp"
#include "irq/irq.hpp"
#include "timer.hpp"
#include "utils.hpp"
#include "virt_console.hpp"
#include <cstdint>
#include <linked_node.hpp>
#include <mem.hpp>
#include <page.hpp>
#include <page_alloc.hpp>
#include <print/printk.hpp>

extern "C" void abort();

#define SYMBOL_ADDRESS(x) (virt_to_page((size_t)x) << 12)

extern "C" {
extern volatile char __boot_start[];
extern volatile char __boot_end[];
extern volatile char __text_start[];
extern volatile char __text_end[];
extern volatile char __data_start[];
extern volatile char __data_end[];
extern volatile char __rodata_start[];
extern volatile char __rodata_end[];
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
    page_allocator.alloc_from_start_end(SYMBOL_ADDRESS(__rodata_start),
                                        SYMBOL_ADDRESS(__rodata_end));
    page_allocator.alloc_from_start_end(SYMBOL_ADDRESS(__bss_start),
                                        SYMBOL_ADDRESS(__bss_end));
    page_allocator.alloc_from_start_end(SYMBOL_ADDRESS(__stack_start),
                                        SYMBOL_ADDRESS(__stack_end));
    page_allocator.alloc_from_ptr(&ttb0_manager);
    page_allocator.alloc_from_ptr(&ttb1_manager);

    auto kernel_l2_pages = page_allocator.reserve_free_range(2);
    l2_page_manager = kernel_l2_pages->make<PageManager<512>>();

    for (size_t index = 0; index < l2_page_manager->size(); ++index) {
        l2_page_manager->modify_page(index, [&](page_descriptor_t &descriptor) {
            descriptor.fields.nblock = 0;
            descriptor.fields.valid = 1;
            descriptor.fields.mem_attrs = 0;
            descriptor.fields.af = 1;
            descriptor.fields.nlta = index << 9 | (0x40000000 >> 12);
        });
    }

    ttb1_manager.modify_page(0, [&](page_descriptor_t &descriptor) {
        descriptor.fields.valid = 1;
        descriptor.fields.nblock = 0;
        descriptor.fields.mem_attrs = 1;
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

void create_heap() {
    constexpr size_t heap_pages = 24;

    auto range = page_allocator.reserve_free_range(heap_pages);

    if (range) {
        ByteAllocator::init_global_allocator(
            std::bit_cast<size_t>(range->start.virt_addr()),
            std::bit_cast<size_t>(range->end.virt_addr()));
    }
}

void timer_handle(void* arg) {
    (void)arg;
    static unsigned int counter = 0;

    early_printk("timer ", counter, "!\n");
    counter++;
    ArmTimer::timer.set_frequency(1);
}

extern "C" {
__attribute__((noinline, used)) void register_exception_handler() {
    uintptr_t exception_addr = (uintptr_t)__exc_vector;

    asm volatile("msr vbar_el1, %0\nisb\n" ::"r"(exception_addr) : "memory");
}
void loop() {
    register_exception_handler();
    early_printk("Setup exception vector.\n");
    build_kernel_pt();
    early_printk("Built kernel page table.\n");
    create_heap();
    early_printk("Built kernel heap.\n");

    Console::create_console((volatile uint32_t *)0xFFFFFF800a003e00);

    Console::print("Hello world!!!\n");

    Gic::create_gic(reinterpret_cast<volatile uint8_t *>(0x8000000),
            reinterpret_cast<volatile uint8_t *>(0x80a0000));
    early_printk("Initialized interrupts.\n");

    ArmTimer &timer = ArmTimer::timer;
    IsrManager::register_isr(27, timer_handle);
    timer.init();
    timer.enable();
    timer.set_delay(10);
    Gic::enable_private_irq(27);
    Gic::set_cpu_prio(0xff);
    Gic::enable_irqs();

    early_printk("Enabling interrupts.\n");
    write_sysreg(daif, 0x0ULL);
    asm volatile("isb" ::: "memory");

    abort();
}

void abort() {
    while (1) {
        asm volatile("wfi");
    }
}
}
