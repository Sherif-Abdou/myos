#pragma once

#include <cstddef>
#include <optional>
#include <utility>

/* All allocations are 8 byte aligned. */
class ByteAllocator {
  private:
    struct Hole {
        size_t size;
        Hole *next;
    };

    struct Cursor {
        Hole **head = nullptr;
        Hole *previous = nullptr;
        Hole *current = nullptr;

        constexpr Hole &operator*();
        constexpr Hole *operator->();

        constexpr void next();
        constexpr void next_while_before(void *addr);
        constexpr void insert(Hole *hole);
        constexpr Hole *remove();
    };

    struct Meta {
        size_t size;
    };

    size_t _start_address;
    size_t _end_address;

    Hole *hole = nullptr;

    void coalesce();
    void *alloc_in(Cursor &, size_t size);
    constexpr Cursor make_cursor();

    static std::optional<ByteAllocator> global_alloc;

  public:
    void init();
    static constexpr size_t byte_alignment = 8;
    ByteAllocator(size_t start_address, size_t end_address)
        : _start_address(start_address), _end_address(end_address) {
        init();
    };

    void *alloc_raw(size_t size);

    template <typename T, typename... Args> T *alloc(Args &&...args) {
        static_assert(alignof(T) < byte_alignment,
                      "Allocator only supports eight byte alignment");
        return new (alloc_raw(sizeof(T))) T(std::forward<Args>(args)...);
    }

    void free(void *ptr);

    static void init_global_allocator(size_t start_address, size_t end_address);
    template <typename T, typename... Args> static T *kalloc(Args &&...args) {
        static_assert(alignof(T) <= byte_alignment,
                      "Allocator only supports eight byte alignment");
        return new (global_alloc->alloc_raw(sizeof(T)))
            T(std::forward<Args>(args)...);
    }

    template <typename T> static void kfree(T *ptr) {
        ptr->~T();
        global_alloc->free(ptr);
    }
};
