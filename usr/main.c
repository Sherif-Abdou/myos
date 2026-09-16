#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>

#include "lib.h"

const char *base = "hello world\n";
static volatile int a;

#define MAX_CMD_LEN 16

ssize_t parse_arg(const char *line, size_t len) {
    int i = 0;
    while (i < len && line[i]) {
        if (line[i] == ' ')
            return i;

        ++i;
    }

    return i;
}

struct fd_redirect {
    int fd;
    const char *path;
    bool output;
};

struct exec_environment {
    int argc;
    const char **argv;
    int redirectc;
    const struct fd_redirect *redirectv;
};

void parse_command(const char *cmd, size_t cmd_len) {
    const char *start = cmd;
    size_t len;

    int argc = 0;

    while (start[0]) {
        len = parse_arg(start, cmd_len);

        start += len;
        if (start[0]) {
            start++;
        }
        argc++;
    }

    if (argc == 0) {
        return;
    }

    char **argv = malloc(sizeof(char *) * argc);
    start = cmd;

    for (int i = 0; i < argc; ++i) {
        len = parse_arg(start, cmd_len);
        argv[i] = malloc((len + 1) * sizeof(char));
        memcpy(argv[i], start, len);
        argv[i][len] = 0;

        start += len;
        if (start[0]) {
            start++;
        }
    }

    int pid = fork();

    if (pid == 0) {
        int ret;
        ret = exec(argv[0], argc, (const char **)argv);
        if (ret < 0) {
            char *new_path = malloc(strlen(argv[0]) + strlen("/bin/") - 1);
            memcpy(new_path, "/bin/", sizeof("/bin/"));
            memcpy(new_path + sizeof("/bin/") - 1, argv[0],
                   strlen(argv[0]) + 1);
            free(argv[0]);
            argv[0] = new_path;
            ret = exec(argv[0], argc, (const char **)argv);
        }
    }

    waitpid(pid);

    for (int i = 0; i < argc; ++i) {
        free(argv[i]);
    }
    free(argv);
}

void shell(void) {
    char line[64];
    line[63] = 0;
    size_t cursor = 0;

    puts("> ");
    while (1) {
        size_t bytes_read = read(0, line + cursor, 63 - cursor);
        if (bytes_read > 0) {
            for (int i = 0; i < bytes_read; ++i) {
                if (line[cursor] == 127) {
                    if (cursor > 0) {
                        putchar('\b');
                        putchar(' ');
                        putchar('\b');
                        cursor -= 1;
                    }
                } else if (line[cursor] == '\r') {
                    putchar('\n');

                    line[cursor] = 0;
                    parse_command(line, cursor);
                    cursor = 0;
                    puts("> ");
                } else {
                    putchar(line[cursor]);
                    cursor += 1;
                }
            }
        }
    }
}

int main(int argc, const char **argv) {
    int fd = open("/dev/console");

    dup2(fd, 0);
    dup2(fd, 1);
    dup2(fd, 2);

    shell();

    return 0;
}
