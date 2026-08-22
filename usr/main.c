#include <stdint.h>

uintptr_t syscall(uintptr_t num, uintptr_t x0, uintptr_t x1, uintptr_t x2,
                  uintptr_t x3, uintptr_t x4, uintptr_t x5, uintptr_t x6,
                  uintptr_t x7) {
    uintptr_t ret;
    __asm__ volatile("mov x8, %1\n"
                     "mov x0, %2\n"
                     "mov x1, %3\n"
                     "mov x2, %4\n"
                     "mov x3, %5\n"
                     "mov x4, %6\n"
                     "mov x5, %7\n"
                     "mov x6, %8\n"
                     "mov x7, %9\n"
                     "svc 1\n"
                     "mov %0, x0\n"
                     : "+r"(ret)
                     : "r"(num), "r"(x0), "r"(x1), "r"(x2), "r"(x3), "r"(x4),
                       "r"(x5), "r"(x6), "r"(x7)
                     : "memory", "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7",
                       "x8");

    return ret;
}

int puts(const char *str) {
    syscall(0, (long long)str, 0, 0, 0, 0, 0, 0, 0);
    return 0;
}

int read(char *str, long len) {
    return syscall(1, (long long)str, len, 0, 0, 0, 0, 0, 0);
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

    char buf[5];
    buf[4] = 0;

    while (1) {
        int bytes_read = read(buf, 4);

        if (bytes_read > 0) {
            puts(buf);
            buf[bytes_read] = 0;
        }
    }

    exit(0);
}
