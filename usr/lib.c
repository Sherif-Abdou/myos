#include <stdint.h>
#include <sys/cdefs.h>
#include <sys/types.h>

#include "lib.h"

size_t strlen(const char *str) {
    size_t i = 0;
    while (str[i] != 0)
        ++i;
    return i;
}

size_t strnlen(const char *str, size_t n) {
    size_t i = 0;
    while (str[i] != 0 && i < n)
        ++i;
    return i;
}

int strncmp(const char *a, const char *b, size_t n) {
    size_t i = 0;
    while (a[i] && b[i] && i < n) {
        if (a[i] != b[i])
            return a[i] - b[i];
        ++i;
    }

    return i < n ? a[i] - b[i] : 0;
}

void *memcpy(void *s1, const void *s2, size_t n) {
    size_t i = 0;

    while (i < n) {
        ((unsigned char *)s1)[i] = ((unsigned char *)s2)[i];
        ++i;
    }
    return s1;
}

void *memset(void *s, int c, size_t n) {
    while (n--)
        ((unsigned char *)s)[n] = (unsigned char)c;
    return s;
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
    return syscall(0, fd, (long long)buf, len, 0, 0, 0, 0, 0);
}

int read(int fd, char *str, long len) {
    return syscall(1, fd, (long long)str, len, 0, 0, 0, 0, 0);
}

int open(const char *path) {
    return syscall(8, (uintptr_t)path, 0, 0, 0, 0, 0, 0, 0);
}

int creat(const char *path) {
    return syscall(9, (uintptr_t)path, 0, 0, 0, 0, 0, 0, 0);
}

int unlink(const char *path) {
    return syscall(25, (uintptr_t)path, 0, 0, 0, 0, 0, 0, 0);
}

int rmdir(const char *path) {
    return syscall(26, (uintptr_t)path, 0, 0, 0, 0, 0, 0, 0);
}

int mkdir(const char *path) {
    return syscall(10, (uintptr_t)path, 0, 0, 0, 0, 0, 0, 0);
}

int chdir(const char *path) {
    return syscall(7, (uintptr_t)path, 0, 0, 0, 0, 0, 0, 0);
}


int exec(const char *path, int argc, const char **argv) {
    return syscall(22, (uintptr_t)path, argc, (uintptr_t)argv, 0, 0, 0, 0, 0);
}

int dup2(int old_fd, int new_fd) {
    return syscall(14, old_fd, new_fd, 0, 0, 0, 0, 0, 0);
}

int close(int fd) { return syscall(11, fd, 0, 0, 0, 0, 0, 0, 0); }

int fork() { return syscall(20, 0, 0, 0, 0, 0, 0, 0, 0); }

int waitpid(int pid) { return syscall(27, pid, 0, 0, 0, 0, 0, 0, 0); }

int truncate(int fd, size_t size) {
    return syscall(6, fd, size, 0, 0, 0, 0, 0, 0);
}

int puts(const char *str) {
    write(1, str, strlen(str));
    return 0;
}

int putchar(int c) {
    char buf = c;

    write(1, &buf, 1);
    return 0;
}

int pipe(int *fds) { return syscall(40, (uintptr_t)fds, 0, 0, 0, 0, 0, 0, 0); }

int ns_sleep(long long delay_ns) {
    syscall(17, delay_ns, 0, 0, 0, 0, 0, 0, 0);

    return 0;
}

void *sbrk(long long offset) {
    return (void *)syscall(33, (uintptr_t)offset, 0, 0, 0, 0, 0, 0, 0);
}

int ms_sleep(long long delay_ms) {
    ns_sleep(delay_ms * 1000000);
    return 0;
}

void exit(int code) {
    syscall(50, code, 0, 0, 0, 0, 0, 0, 0);
    __builtin_unreachable();
}

int getdents(int fd, char *buf, size_t len) {
    return syscall(24, fd, (uintptr_t)buf, len, 0, 0, 0, 0, 0);
}

int stat(const char *path, struct stat *stat) {
    return syscall(12, (uintptr_t)path, (uintptr_t)stat, 0, 0, 0, 0, 0, 0);
}

__attribute__((weak)) int main(int argc, const char **argv);

int _start() {
    unsigned long argc;
    const char **argv;
    __asm__ volatile("mov %0, x0\n"
                     "mov %1, x1\n"
                     : "=r"(argc), "=r"(argv)::"x0", "x1");

    int ret = main(argc, argv);

    exit(ret);
}

struct alloc_ll_header {
    // Memory in this header (including the header).
    size_t size;
    struct alloc_ll_header *next;
};

#define ALLOC_ALIGN 16

struct alloc_ll_header *claim_more_mem(size_t hint) {
    size_t growth =
        hint > 4096 ? (hint + ALLOC_ALIGN - 1) & ~(ALLOC_ALIGN - 1) : 4096;
    struct alloc_ll_header *ptr = sbrk(0);
    sbrk(growth);

    ptr->size = growth;
    ptr->next = NULL;

    return ptr;
}

static struct alloc_ll_header *first_hole = NULL;

void *malloc(size_t size) {
    // Align up allocation size.
    size = (size + ALLOC_ALIGN - 1) & ~(ALLOC_ALIGN - 1);

    if (size == 0)
        return NULL;

    // claim more memory none currently available.
    if (first_hole == NULL)
        first_hole = claim_more_mem(size + sizeof(struct alloc_ll_header));

    struct alloc_ll_header *prev = NULL;
    struct alloc_ll_header *hole = first_hole;
    size_t mem_needed =
        size + (sizeof(size_t) + (ALLOC_ALIGN - 1)) & ~(ALLOC_ALIGN - 1);
    while (hole && hole->size < mem_needed) {
        prev = hole;
        hole = hole->next;
    }

    if (!hole) {
        hole = claim_more_mem(size + sizeof(struct alloc_ll_header));

        if (prev)
            prev->next = hole;
        else
            first_hole = hole;
    }

    struct alloc_ll_header *hole_next = hole->next;

    uintptr_t mem_start =
        ((uintptr_t)hole + sizeof(size_t) + (ALLOC_ALIGN - 1)) &
        ~(ALLOC_ALIGN - 1);
    uintptr_t header_start = (uintptr_t)hole;
    uintptr_t header_end = header_start + mem_needed;
    size_t remaining = hole->size - mem_needed;

    *(size_t *)header_start = remaining >= sizeof(struct alloc_ll_header)
                                  ? mem_needed
                                  : mem_needed + remaining;

    if (remaining >= sizeof(struct alloc_ll_header)) {
        struct alloc_ll_header *end = (struct alloc_ll_header *)header_end;

        if (prev)
            prev->next = end;
        else
            first_hole = end;
        end->size = remaining;
        end->next = hole_next;
    } else {
        if (prev)
            prev->next = hole_next;
        else
            first_hole = hole_next;
    }

    return (void *)mem_start;
}

void free(void *ptr) {
    if (ptr == NULL)
        return;

    uintptr_t header_addr =
        ((uintptr_t)ptr - sizeof(size_t)) & ~(ALLOC_ALIGN - 1);
    struct alloc_ll_header *header = (struct alloc_ll_header *)header_addr;

    struct alloc_ll_header *prev = NULL;
    struct alloc_ll_header *current = first_hole;
    while (current && (uintptr_t)current < header_addr) {
        prev = current;
        current = current->next;
    }

    header->next = prev ? prev->next : first_hole;
    if (prev)
        prev->next = header;
    else
        first_hole = prev;

    uintptr_t end_of_current;
    while (current && (end_of_current = (uintptr_t)current + current->size) ==
                          (uintptr_t)current->next) {
        current->size += current->next->size;
        current->next = current->next->next;
    }

    current = prev;
    while (current && (end_of_current = (uintptr_t)current + current->size) ==
                          (uintptr_t)current->next) {
        current->size += current->next->size;
        current->next = current->next->next;
    }
}
