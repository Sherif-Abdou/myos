#include <stddef.h>
#include <stdint.h>

size_t strlen(const char *str) {
    size_t i = 0;
    while (str[i] != 0)
        ++i;
    return i;
}

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
    syscall(0, (long long)str, strlen(str), 0, 0, 0, 0, 0, 0);
    return 0;
}

int write(const char *buf, size_t len) {
    syscall(0, (long long)buf, len, 0, 0, 0, 0, 0, 0);
    return 0;
}

int putchar(char c) {
    syscall(0, (long long)&c, 1, 0, 0, 0, 0, 0, 0);
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
    puts(base);

    char line[64];
    line[63] = 0;
    size_t cursor = 0;

    puts("# ");
    while (1) {
        size_t bytes_read = read(line + cursor, 63 - cursor);
        if (bytes_read > 0) {
            for (int i = 0; i < bytes_read; ++i) {
                if (line[cursor] == 127) {
                    if (cursor > 0) {
                        putchar('\b');
                        cursor -= 1;
                    }
                } else if (line[cursor] == '\r') {
                    putchar('\n');
                    write(line, cursor);
                    putchar('\n');
                    cursor = 0;
                    puts("# ");
                } else {
                    putchar(line[cursor]);
                    cursor += 1;
                }
            }
        }
    }

    exit(0);
}
