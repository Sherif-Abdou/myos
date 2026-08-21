#include <stdint.h>

void syscall(uintptr_t num, uintptr_t x0, uintptr_t x1, uintptr_t x2,
             uintptr_t x3, uintptr_t x4, uintptr_t x5, uintptr_t x6,
             uintptr_t x7) {
    __asm__ volatile("mov x8, %0\n"
                     "mov x0, %1\n"
                     "mov x1, %2\n"
                     "mov x2, %3\n"
                     "svc 1\n" ::"r"(num),
                     "r"(x0), "r"(x1), "r"(x2)
                     : "memory", "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7",
                       "x8");
}

int puts(const char *str) {
    syscall(27, (long long)str, 0, 0, 0, 0, 0, 0, 0);
    return 0;
}

void exit(int code) {
    syscall(50, code, 0, 0, 0, 0, 0, 0, 0);
    __builtin_unreachable();
}

const char *base = "hello world\n";

void _start(void) {
    int a[20];

    a[0] = 'h';
    a[1] = '\n';
    a[2] = 0;

    puts(base);

    exit(0);
}
