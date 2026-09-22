#include <stddef.h>
#include <stdint.h>

struct stat {
    uint32_t st_dev;
    uint32_t st_ino;
    uint32_t st_mode;
    uint32_t st_nlink;
    uint32_t st_uid;
    uint32_t st_gid;
    uint32_t st_rdev;
    uint32_t st_size;
    uint32_t st_atime;
    uint32_t st_mtime;
    uint32_t st_ctime;
    uint32_t st_blocksize;
    uint32_t st_blocks;
};

size_t strlen(const char *str);
size_t strnlen(const char *str, size_t n);
int strncmp(const char *a, const char *b, size_t n);

char *strtok(char *str, const char *delimiters);

void *memcpy(void *s1, const void *s2, size_t n);

void *memset(void *s, int c, size_t n);

uintptr_t syscall(uintptr_t num, uintptr_t x0, uintptr_t x1, uintptr_t x2,
                  uintptr_t x3, uintptr_t x4, uintptr_t x5, uintptr_t x6,
                  uintptr_t x7);

int write(int fd, const char *buf, size_t len);

int read(int fd, char *str, long len);

int open(const char *path);

int creat(const char *path);

int exec(const char *path, int argc, const char **argv);

int dup2(int old_fd, int new_fd);

int close(int fd);

int fork();

int waitpid(int pid);

int puts(const char *str);

int putchar(int c);

int pipe(int fds[2]);

int getdents(int fd, char *buf, size_t len);

int ns_sleep(long long delay_ns);

int ms_sleep(long long delay_ms);

void *sbrk(long long offset);

void exit(int code);

int unlink(const char *path);

int rmdir(const char *path);

int mkdir(const char *path);

int chdir(const char *path);

void *malloc(size_t size);

void free(void *ptr);

int truncate(int fd, size_t size);

int stat(const char *path, struct stat *stat);
