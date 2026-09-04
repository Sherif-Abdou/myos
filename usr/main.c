#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include "lib.h"

const char *base = "hello world\n";

void shell(void) {
    char line[64];
    line[63] = 0;
    size_t cursor = 0;

    puts("# ");
    while (1) {
        size_t bytes_read = read(1, line + cursor, 63 - cursor);
        if (bytes_read > 0) {
            for (int i = 0; i < bytes_read; ++i) {
                if (line[cursor] == 127) {
                    if (cursor > 0) {
                        putchar('\b');
                        cursor -= 1;
                    }
                } else if (line[cursor] == '\r') {
                    putchar('\n');
                    write(0, line, cursor);
                    putchar('\n');
                    cursor = 0;
                    puts("# ");
                } else {
                    putchar(line[cursor]);
                    cursor += 1;
                }
            }
        }
    }
}

int main(int argc, const char **argv) {
    int child = fork();
    if (child != 0) {
        const char *addr = "hello land\n";
        // write(0, (const char *)0x8, 4);

        int fd = open("hi.txt");

        char buf[64];
        buf[63] = 0;

        if (fd < 0) {
            puts("ahhh\n");
        } else {
            int len = read(fd, buf, 63);

            puts(buf);
            close(fd);
        }
        waitpid(child);

        puts("This is the parent after the child is done.\n");
    } else {
        const char *buf[2];
        buf[0] = "this is an argument\n";
        buf[1] = "so guys, thoughts on markiplier?\n";
        exec("main", 2, buf);
    }

    return 0;
}
