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

int write(int fd, const char *buf, size_t len) {
    syscall(0, fd, (long long)buf, len, 0, 0, 0, 0, 0);
    return 0;
}

int read(int fd, char *str, long len) {
    return syscall(1, fd, (long long)str, len, 0, 0, 0, 0, 0);
}

int open(const char *path) {
    return syscall(8, (uintptr_t)path, 0, 0, 0, 0, 0, 0, 0);
}

int exec(const char *path, int argc, const char ** argv) {
    return syscall(22, (uintptr_t)path, argc, (uintptr_t)argv, 0, 0, 0, 0, 0);
}

int close(int fd) { return syscall(11, fd, 0, 0, 0, 0, 0, 0, 0); }

int fork() { return syscall(20, 0, 0, 0, 0, 0, 0, 0, 0); }

int waitpid(int pid) { return syscall(27, pid, 0, 0, 0, 0, 0, 0, 0); }

int puts(const char *str) {
    write(0, str, strlen(str));
    return 0;
}

int putchar(int c) {
    char buf = c;

    write(0, &buf, 1);
    return 0;
}

int ns_sleep(long long delay_ns) {
    syscall(17, delay_ns, 0, 0, 0, 0, 0, 0, 0);

    return 0;
}

void *sbrk(long long offset) {
    return (void*)syscall(33, (uintptr_t)offset, 0, 0, 0, 0, 0, 0, 0);

}

int ms_sleep(long long delay_ms) { 
    ns_sleep(delay_ms * 1000000); 
    return 0;
}

void exit(int code) {
    syscall(50, code, 0, 0, 0, 0, 0, 0, 0);
    __builtin_unreachable();
}

__attribute__((weak)) int main(int argc, const char **argv);

int _start() {
    int argc; const char **argv;
    __asm__ volatile (
        "mov %0, x0\n"
        "mov %1, x1\n"
        : "=r"(argc), "=r"(argv) :: "x0", "x1"
    );

    int ret = main(argc, argv);

    exit(ret);
}
