#include "lib.h"

int main(int argc, const char **argv) {
    const char *path = argv[1];

    char *buf = malloc(1024);
    int fd = open(path);
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
