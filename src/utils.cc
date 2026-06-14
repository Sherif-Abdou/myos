#include <stddef.h>

void isb() { asm volatile("isb" ::: "memory"); }

extern "C" void *memset(void *dst, int c, size_t n) {
    auto *p = static_cast<unsigned char *>(dst);
    while (n--)
        *p++ = static_cast<unsigned char>(c);
    return dst;
}

extern "C" void *memcpy(void *dst, const void *src, size_t n) {
    auto *d = static_cast<unsigned char *>(dst);
    auto *s = static_cast<const unsigned char *>(src);
    while (n--)
        *d++ = *s++;
    return dst;
}

extern "C" void *memmove(void *dst, const void *src, size_t n) {
    auto *d = static_cast<unsigned char *>(dst);
    auto *s = static_cast<const unsigned char *>(src);
    if (d < s)
        while (n--)
            *d++ = *s++;
    else {
        d += n;
        s += n;
        while (n--)
            *--d = *--s;
    }
    return dst;
}

extern "C" size_t strlen(const char *src) {
    size_t size = 0;
    while (*(src++))
        ++size;
    return size;
}
