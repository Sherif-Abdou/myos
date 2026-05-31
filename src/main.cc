#include <page.hpp>
#include <page_alloc.hpp>

extern "C" void abort();

constinit static PageManager<4> ttb0_manager{};
constinit static PageManager<4> ttb1_manager{};

constinit static PageAllocator page_allocator{};

#define SYMBOL_ADDRESS(x) (virt_to_page((size_t)x))

extern "C" {
extern char __boot_start[];
extern char __boot_end[];
extern char __text_start[];
extern char __text_end[];
extern char __data_start[];
extern char __data_end[];
extern char __bss_start[];
extern char __bss_end[];
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
    page_allocator.alloc_from_ptr(&ttb0_manager);
    page_allocator.alloc_from_ptr(&ttb1_manager);

    auto kernel_l2_pages = page_allocator.reserve_free_range(2);
    auto kernel_l3_pages = page_allocator.reserve_free_range(512 + 1);
    auto l2_page_manager =
        reinterpret_cast<PageManager<512> *>(kernel_l2_pages->start);
    auto *l3_page_manager =
        reinterpret_cast<std::array<PageManager<512>, 512> *>(
            kernel_l3_pages->start);

    for (size_t index = 0; index < l2_page_manager->size(); ++index) {
        l2_page_manager->modify_page(index, [&](page_descriptor_t &descriptor) {
            descriptor.fields.nblock = 1;
            descriptor.fields.valid = 1;
            descriptor.fields.mem_attrs = 0;
            descriptor.fields.nlta = virt_to_page(
                std::bit_cast<size_t>(&((*l3_page_manager)[index])));
        });
    }
    for (size_t l2_block = 0; l2_block < l3_page_manager->size(); ++l2_block) {
        auto l3_page = &((*l3_page_manager)[l2_block]);
        for (size_t index = 0; index < l3_page->size(); ++index) {
            auto page = l2_block * 512 + index;
            l3_page->modify_page(index, [&](page_descriptor_t &descriptor) {
                descriptor.fields.nblock = 0;
                descriptor.fields.valid = !page_allocator.is_free(page);
                descriptor.fields.mem_attrs = 0;
                descriptor.fields.nlta = page;
            });
        }
    }

    ttb1_manager.modify_page(0, [&](page_descriptor_t &descriptor) {
        descriptor.fields.valid = 1;
        descriptor.fields.nblock = 1;
        descriptor.fields.nlta =
            virt_to_page(std::bit_cast<size_t>(l2_page_manager));
    });

    auto ttbr_addr = virt_to_page(reinterpret_cast<size_t>(&ttb1_manager))
                     << 12;

    asm volatile(R"(
        tlbi vmalle1
        dsb ish
        isb
        msr ttbr1_el1, %0
        isb
    )" ::"r"(ttbr_addr));
}

extern "C" {
void loop() {
    build_kernel_pt();
    abort();
}

void abort() {
    while (1) {
        asm volatile("wfi");
    }
}
}
