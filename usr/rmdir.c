#include "lib.h"

int main(int argc, const char **argv) {
    const char *path = argv[1];
    int ret = rmdir(path);
    
    return ret;
}
