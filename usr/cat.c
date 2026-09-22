#include "lib.h"

int main(int argc, const char **argv) {
    int fd;
    if (argc == 2) {
        const char *path = argv[1];
        fd = open(path);
    } else {
        // Otherwise, just read stdin.
        fd = 0;
    }

    char *buf = malloc(1024);
    int ret;
    if (fd < 0)
        return fd;

    do {
        ret = read(fd, buf, 1024);
        write(1, buf, ret);
    } while (ret > 0);

teardown:
    close(fd);
    free(buf);

    return ret;
}
