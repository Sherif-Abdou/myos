#include "print/printk.hpp"
#include <cstdint>

#define BASE ((volatile char *)0x9000000)

void putchar_early(char c) {
    // Wait for transmit fifo to not be full
    while (*(BASE + 0x18) & bit(5U));
    *BASE = c;
}

void early_printk_item(const char *buf) {
    size_t len = strlen(buf);

    for (size_t i = 0; i < len; ++i)
        putchar_early(buf[i]);
};

void early_printk_item(uint8_t number) {
    if (number == 0) {
        putchar_early('0');
    }
    char buffer[20];
    int i = 0;
    while (number > 0) {
        char digit = (number % 10) + '0';
        buffer[i] = digit;
        i += 1;
        number /= 10;
    }

    i-=1;
    for (; i >= 0; --i)
        putchar_early(buffer[i]);
}

void early_printk_item(uint16_t number) {
    if (number == 0) {
        putchar_early('0');
    }
    char buffer[20];
    int i = 0;
    while (number > 0) {
        char digit = (number % 10) + '0';
        buffer[i] = digit;
        i += 1;
        number /= 10;
    }

    i-=1;
    for (; i >= 0; --i)
        putchar_early(buffer[i]);
}

void early_printk_item(uint32_t number) {
    if (number == 0) {
        putchar_early('0');
    }
    char buffer[20];
    int i = 0;
    while (number > 0) {
        char digit = (number % 10) + '0';
        buffer[i] = digit;
        i += 1;
        number /= 10;
    }

    i-=1;
    for (; i >= 0; --i)
        putchar_early(buffer[i]);
}

void early_printk_item(uint64_t number) {
    if (number == 0) {
        putchar_early('0');
    }
    char buffer[20];
    int i = 0;
    while (number > 0) {
        char digit = (number % 10) + '0';
        buffer[i] = digit;
        i += 1;
        number /= 10;
    }

    i-=1;
    for (; i >= 0; --i)
        putchar_early(buffer[i]);
}
