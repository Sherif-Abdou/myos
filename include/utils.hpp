#pragma once
#include <cstddef>

#define MOVE_ONLY(name) \
    name(const name&) = delete; \
    name& operator=(const name&) = delete; \
    name& operator=(name&&) = default; \
    name(name&&) = default;

template <typename T>
T align_up(T value, size_t alignment) {
    return (value + (alignment - 1)) & ~(alignment - 1);
}
