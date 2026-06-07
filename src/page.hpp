#pragma once

#include "utils.hpp"
#include <array>
#include <cstddef>
#include <cstdint>
#include <type_traits>

struct page_descriptor_t {
    union {
        struct {
            uint64_t valid : 1;
            uint64_t nblock : 1;
            uint64_t mem_attrs : 4;
            uint64_t s2 : 2;
            // Sharability
            uint64_t sh : 2;
            uint64_t af : 1;
            uint64_t : 1;
            // Next level address, either for a page/block or for a lower layer
            uint64_t nlta : 36;
            uint64_t : 11;
            uint64_t upper_attrs : 5;
        } fields;
        uint64_t raw;
    };

    page_descriptor_t() = default;
    page_descriptor_t(const page_descriptor_t &) = default;
    page_descriptor_t &operator=(const page_descriptor_t &) = default;
    page_descriptor_t(page_descriptor_t &&) = default;
    page_descriptor_t &operator=(page_descriptor_t &&) = default;
};

static_assert(sizeof(page_descriptor_t) == 8);

class Page {
  private:
    size_t page_number_;

  public:
    constexpr Page(size_t page_number_) : page_number_(page_number_) {}

    constexpr size_t page_number() const { return this->page_number_; }

    void *virt_addr() const {
        return reinterpret_cast<void *>(0xFFFFFF8040000000 |
                                        (page_number() << 12));
    };

    size_t phys_addr() const { return page_number() << 12; }
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

    template <typename F>
    void modify_and_apply_page(size_t index, F transform) {
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

    constexpr size_t size() const { return table.size(); }
};
