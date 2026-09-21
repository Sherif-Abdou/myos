#include "lib.h"

int main(int argc, const char **argv) {
    for (int i = 1; i < argc; ++i) {
        puts(argv[i]);
        putchar(' ');
    }
    putchar('\n');
    
    return 0;
}
