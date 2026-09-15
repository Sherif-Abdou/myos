#include <stddef.h>
#include <stdint.h>
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

int run_cat(const char *args) {
    const char *path = args;
    char buf[16] = {0};
    int fd = open(path);
    int ret;
    if (fd < 0)
        return fd;

    do {
        ret = read(fd, buf, 16);
        write(1, buf, ret);
    } while (ret > 0);

teardown:
    close(fd);

    return ret;
};

int run_ls(const char *args) {
    const char *path = args;
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
};

int run_command(const char *cmd) {
    int cmd_len = 0;
    while (cmd_len < MAX_CMD_LEN && cmd[cmd_len] != ' ')
        ++cmd_len;

    if (cmd_len == 2 && strncmp(cmd, "ls", 2)) {
        run_ls(cmd + cmd_len + 1);
    }
    if (cmd_len == 3 && strncmp(cmd, "cat", 3)) {
        run_cat(cmd + cmd_len + 1);
    }
    if (cmd_len == 4 && strncmp(cmd, "echo", 4)) {
        puts(cmd + cmd_len + 1);
        putchar('\n');
    }

    return -1;
}

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

    char **argv = malloc(sizeof(char*) * argc);
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
        exec(argv[0], argc, (const char**)argv);
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

    puts("# ");
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
    int fd = open("/dev/console");

    dup2(fd, 0);
    dup2(fd, 1);
    dup2(fd, 2);

    shell();

    return 0;
}
