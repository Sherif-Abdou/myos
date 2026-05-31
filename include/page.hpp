#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <type_traits>
#include <utils.hpp>

struct page_descriptor_t {
    union {
        struct {
            unsigned short valid : 1;
            unsigned short nblock : 1;
            unsigned short mem_attrs : 4;
            unsigned short s2 : 2;
            // Sharability
            unsigned short sh : 2;
            unsigned short af : 1;
            unsigned short : 1;
            // Next level address, either for a page/block or for a lower layer
            unsigned long nlta : 32;
            unsigned short : 11;
            unsigned short upper_attrs : 5;
        } fields;
        uint64_t raw;
    };

    page_descriptor_t() = default;
    page_descriptor_t(const page_descriptor_t &) = default;
    page_descriptor_t &operator=(const page_descriptor_t &) = default;
    page_descriptor_t(page_descriptor_t &&) = default;
    page_descriptor_t &operator=(page_descriptor_t &&) = default;
};

template <size_t N> class PageManager {
  private:
    // An invalid page to follow break before make
    static constexpr page_descriptor_t invalid_page{};

    alignas(4096) std::array<page_descriptor_t, N> table;
    // Keep an extra page as a temporary page so that
    // modifications can be done.
    page_descriptor_t swap_page;

    static constexpr size_t l1_mask = 0x1FF << 30;
    static constexpr size_t l2_mask = 0x1FF << 21;
    static constexpr size_t l3_mask = 0x1FF << 12;

  public:
    static constexpr size_t ppage = 12;
    static constexpr size_t l1_size = 1 << 30;
    static constexpr size_t l2_size = 1 << 21;
    static constexpr size_t l3_size = 1 << 12;

    explicit constexpr PageManager() : table{}, swap_page{} {
        for (page_descriptor_t &page : table) {
            page = invalid_page;
        }
    };

    MOVE_ONLY(PageManager);

    template <typename F> void modify_and_apply_page(size_t index, F transform) {
        static_assert(std::is_invocable_v<F, page_descriptor_t &>,
                      "Transform must be a function.");

        volatile page_descriptor_t *target_page = &table[index];

        swap_page.raw = target_page->raw;
        transform(swap_page);
        target_page->raw = invalid_page.raw;
        asm volatile(R"(
            tlbi vmalle1
            dsb ish
            isb
        )");
        target_page->raw = swap_page.raw;
        asm volatile(R"(
            isb
        )");
    }

    template <typename F> void modify_page(size_t index, F transform) {
        static_assert(std::is_invocable_v<F, page_descriptor_t &>,
                      "Transform must be a function.");

        volatile page_descriptor_t *target_page = &table[index];

        swap_page.raw = target_page->raw;
        transform(swap_page);
        target_page->raw = swap_page.raw;
    }

    constexpr size_t virt_to_phys(size_t addr) const {
        addr = addr & 0x7FFFFFFFFF;
        size_t l1_index = addr >> 30;
        const page_descriptor_t &l1_table = this->table[l1_index];

        if (!l1_table.fields.nblock)
            return (addr & !l1_mask) | (l1_table.fields.nlta << 30);

        return 0;
    }

    constexpr size_t size() const { return table.size(); }
};
