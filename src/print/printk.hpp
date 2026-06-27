#pragma once

#include "utils.hpp"
#include <cstdint>
#include <type_traits>

template<typename T>
struct Hex {
    T inner;
};

void putchar_early(char c);

template<typename T>
void early_printk_item(Hex<T> number) {
    static const char *lut = "0123456789abcdef";
    if (number.inner == 0) {
        putchar_early('0');
    }
    char buffer[8];
    int i = 0;
    while (number.inner > 0) {
        char digit = (number.inner % 16);
        buffer[i] = lut[digit];
        i += 1;
        number.inner /= 16;
    }
    i-=1;
    for (; i >= 0; --i)
        putchar_early(buffer[i]);
}

void early_printk_item(uint8_t number);
void early_printk_item(uint16_t number);
void early_printk_item(uint32_t number);
void early_printk_item(uint64_t number);

void early_printk_item(const char *);

template<typename ...Args>
void early_printk(Args... args) {
    ((early_printk_item(args)), ...);
}
