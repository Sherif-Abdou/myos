#include <stddef.h>
#include <stdint.h>

size_t strlen(const char *str);

uintptr_t syscall(uintptr_t num, uintptr_t x0, uintptr_t x1, uintptr_t x2,
                  uintptr_t x3, uintptr_t x4, uintptr_t x5, uintptr_t x6,
                  uintptr_t x7);

int write(int fd, const char *buf, size_t len);

int read(int fd, char *str, long len);

int open(const char *path);

int exec(const char *path, int argc, const char ** argv);

int close(int fd);

int fork();

int waitpid(int pid);

int puts(const char *str);

int putchar(int c);

int ns_sleep(long long delay_ns);

int ms_sleep(long long delay_ms);

void *sbrk(long long offset);

void exit(int code);
