#include "lib.h"

int main(int argc, const char **argv) {
    const char *path = argv[1];
    struct stat stats;
    int ret = stat(path, &stats);

    return ret;
}
