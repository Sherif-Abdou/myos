#pragma once
#include <cstddef>

#define read_sysreg(x, name)                                                   \
    do {                                                                       \
        asm volatile("mrs %0, " #name : "=r"(x));                              \
    } while (0)

#define write_sysreg(name, x)                                                  \
    do {                                                                       \
        asm volatile("msr " #name ", %0" ::"r"(x));                            \
    } while (0)

#define MOVE_ONLY(name)                                                        \
    name(const name &) = delete;                                               \
    name &operator=(const name &) = delete;                                    \
    name &operator=(name &&) = default;                                        \
    name(name &&) = default;

template <typename T> T align(T value, size_t alignment) {
    return (value + (alignment - 1)) & ~(alignment - 1);
}

template <typename T> constexpr T virt_to_phys(T addr) {
    addr = addr & 0x7FFFFFFFFF;
    return addr;
}

extern "C" void *memset(void *dst, int c, size_t n);

extern "C" void *memcpy(void *dst, const void *src, size_t n);

extern "C" void *memmove(void *dst, const void *src, size_t n);

extern "C" size_t strlen(const char *src);
