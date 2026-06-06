#pragma once
#include "page.hpp"
#include <array>
#include <bit>
#include <cstddef>
#include <cstdint>
#include <optional>

template <typename T> constexpr T virt_to_page(T addr) {
    return (addr & 0x7FFFFFFFFF) >> 12;
}

template <std::size_t N> struct alignas(4096) PageBitmask {
    std::array<uint8_t, (N * 4096)> bitmask;

    constexpr PageBitmask() : bitmask{} {
        for (auto &byte : bitmask)
            byte = 0;
    }

    PageBitmask(const PageBitmask &) = delete;
    PageBitmask &operator=(const PageBitmask &) = delete;
    PageBitmask(PageBitmask &&) = delete;
    PageBitmask &operator=(PageBitmask &&) = delete;

    constexpr bool is_free(size_t page) const {
        auto index = page / 8;
        auto offset = page % 8;
        return (bitmask[index] & (1 << offset)) == 0;
    }
    constexpr void free(size_t page) {
        auto index = page / 8;
        auto offset = page % 8;
        bitmask[index] &= ~(1 << offset);
    }
    constexpr void alloc(size_t page) {
        auto index = page / 8;
        auto offset = page % 8;
        bitmask[index] |= 1 << offset;
    }

    constexpr size_t size() { return N * 4096; }

    constexpr size_t page_count() { return size() * 8; };
};

class PageAllocator {
  private:
    static constexpr size_t bitmask_pages = 32;
    // Supports up to four GB of physical address space.
    PageBitmask<bitmask_pages> page_bitmask;

  public:
    struct PageRange {
        Page start;
        Page end;

        constexpr size_t page_count() {
            return end.page_number() - start.page_number();
        }

        template <typename T, typename... Args> T* make(Args &&...args) {
            return new (start.virt_addr()) T(std::forward<Args>(args)...);
        }
    };
    constexpr PageAllocator() : page_bitmask{} {}

    void reserve_local_pages() { alloc_from_ptr(this); }

    template <typename T> void alloc_from_ptr(const T *ptr) {
        const size_t page_count = (sizeof(T) + 4095) / 4096;
        const size_t base = virt_to_page(std::bit_cast<size_t>(ptr));

        for (size_t i = 0; i < page_count; ++i) {
            page_bitmask.alloc(base + i);
        }
    }

    void alloc_from_start_end(const size_t start, const size_t end) {
        const size_t base = start / 4096;
        const size_t page_count = ((end - start) + 4095) / 4096;

        for (size_t i = 0; i < page_count; ++i) {
            page_bitmask.alloc(base + i);
        }
    }

    constexpr void alloc(size_t page) { this->page_bitmask.alloc(page); }

    constexpr void free(size_t page) { this->page_bitmask.free(page); }

    constexpr bool is_free(size_t page) {
        return this->page_bitmask.is_free(page);
    }

    constexpr bool is_free(const Page &page) {
        return this->is_free(page.page_number());
    }

    constexpr std::optional<size_t> find_first_free() {
        for (size_t index = 0; index < page_bitmask.size(); ++index) {
            if (page_bitmask.bitmask[index] != 0xFF) {
                size_t offset = std::countr_one(page_bitmask.bitmask[index]);
                return {index * 8 + offset};
            }
        }
        return std::nullopt;
    }

    // Attempts to reserve a continuous range of pages.
    constexpr std::optional<PageRange> reserve_free_range(size_t range_size) {
        PageRange range{0, 0};
        for (size_t page = 0; page < page_bitmask.page_count(); ++page) {
            if (range.page_count() >= range_size)
                return {range};
            if (page_bitmask.is_free(page)) {
                range.end = Page(range.end.page_number() + 1);
            } else {
                range.start = {page + 1};
                range.end = {page + 1};
            }
        }
        return std::nullopt;
    }
};
