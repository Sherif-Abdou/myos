#include <cstddef>

/* All allocations are 8 byte aligned. */
class ByteAllocator {
  private:
    struct Hole {
        size_t size;
        Hole *next;
    };

    struct Cursor {
        Hole **head;
        Hole *previous;
        Hole *current;

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

    Hole *hole;

    void init();
    void coalesce();
    void *alloc_in(Cursor &, size_t size);
    constexpr Cursor make_cursor();

  public:
    static constexpr size_t byte_alignment = 8;
    ByteAllocator(size_t start_address, size_t end_address)
        : _start_address(start_address), _end_address(end_address) {
        init();
    };

    void *alloc(size_t size);
    void free(void* ptr);
};
