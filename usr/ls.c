#include "lib.h"

int main(int argc, const char **argv) {
    const char *path = argc > 1 ? argv[1] : "";

    char buf[64];
    int fd = open(path);
    int ret = 0;
    if (fd < 0)
        return fd;

    int count = getdents(fd, buf, 64);
    if (count < 0) {
        goto teardown;
    }
    const char *name = buf;
    do {
        int len = strlen(name);

        write(1, name, len);
        putchar('\n');

        name = name + len + 1;
        count -= len + 1;
    } while (count > 0);

teardown:
    close(fd);

    return ret;
}
