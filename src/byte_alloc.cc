#include <byte_alloc.hpp>
#include <utils.hpp>

constexpr ByteAllocator::Hole &ByteAllocator::Cursor::operator*() {
    return *this->current;
}

constexpr ByteAllocator::Hole *ByteAllocator::Cursor::operator->() {
    return this->current;
}

constexpr void ByteAllocator::Cursor::next() {
    previous = current;
    current = current->next;
}

constexpr void ByteAllocator::Cursor::next_while_before(void *addr) {
    while (current && current < addr)
        next();
}

constexpr void ByteAllocator::Cursor::insert(Hole *hole) {
    if (previous)
        previous->next = hole;
    else
        *head = hole;

    hole->next = current;
    current = hole;
}

constexpr ByteAllocator::Hole *ByteAllocator::Cursor::remove() {
    if (previous)
        previous->next = current->next;
    else
        *head = current->next;
    Hole *tmp = current;
    current = current->next;

    return tmp;
}

constexpr ByteAllocator::Cursor ByteAllocator::make_cursor() {
    return Cursor{
        .head = &hole,
        .previous = nullptr,
        .current = hole,
    };
}

void *ByteAllocator::alloc_in(Cursor &cursor, size_t size) {
    size_t hole_size = cursor->size;
    Meta *header_addr = reinterpret_cast<Meta *>(cursor.remove());
    header_addr->size = size;
    void *data_addr = reinterpret_cast<void *>(header_addr + 1);
    if (hole_size > sizeof(Meta) + size) {
        Hole *new_hole = reinterpret_cast<Hole *>(
            reinterpret_cast<char *>(data_addr) + size);
        new_hole->size = hole_size - sizeof(Meta) - size;
        cursor.insert(new_hole);
    }

    coalesce();

    return data_addr;
}

void ByteAllocator::init() {
    Hole *first_hole = reinterpret_cast<Hole *>(_start_address);
    first_hole->size = _end_address - _start_address;
    first_hole->next = nullptr;
}

void *ByteAllocator::alloc(size_t size) {
    size = align(size, byte_alignment);
    size_t effective_size = size + sizeof(Meta);
    auto cursor = make_cursor();
    /* First fit search. */
    while (cursor.current && cursor->size < effective_size)
        cursor.next();

    if (cursor.current)
        return alloc_in(cursor, size);
    else
        return nullptr;
}

void ByteAllocator::free(void *ptr) {
    Meta *meta = reinterpret_cast<Meta *>(ptr) - 1;
    size_t size = meta->size;
    Hole *hole = reinterpret_cast<Hole *>(meta);
    hole->size = size + sizeof(Meta);

    auto cursor = make_cursor();
    cursor.next_while_before(hole);
    cursor.next();
    cursor.insert(hole);

    coalesce();
}

void ByteAllocator::coalesce() {
    Hole *start = hole;
    auto cursor = make_cursor();
    cursor.next();
    while (cursor.current) {
        if ((reinterpret_cast<char *>(cursor.current) -
             reinterpret_cast<char *>(start)) ==
            static_cast<ptrdiff_t>(start->size)) {
            auto old_hole = cursor.remove();
            start->size += old_hole->size;
        } else {
            start = cursor.current;
            cursor.next();
        }
    }
}
