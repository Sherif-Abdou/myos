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

void parse_command(const char *cmd, size_t cmd_len) {
    const char *start = cmd;
    size_t len;
    int i;

    int arg_count = 0;

    while (start[0]) {
        len = parse_arg(start, cmd_len);

        start += len;
        if (start[0]) {
            start++;
        }
        arg_count++;
    }

    if (arg_count == 0) {
        return;
    }

    int argc = 0;

    char **argv = malloc(sizeof(char *) * arg_count);
    start = cmd;

    for (i = 0; i < arg_count; ++i) {
        len = parse_arg(start, cmd_len);
        if ((strncmp(start, ">", len) == 0) ||
            (strncmp(start, "<", len) == 0)) {
            break;
        }
        argv[i] = malloc((len + 1) * sizeof(char));
        memcpy(argv[i], start, len);
        argv[i][len] = 0;
        argc++;

        start += len;
        if (start[0]) {
            start++;
        }
    }
    char *output_redirect_path = NULL;
    char *input_redirect_path = NULL;

    // Redirections spotted
    while (i < arg_count) {
        len = parse_arg(start, cmd_len);
        if (strncmp(start, ">", len) == 0) {
            free(output_redirect_path);

            start += len;
            if (start[0]) {
                start++;
            }
            len = parse_arg(start, cmd_len);
            ++i;
            if (len == 0)
                goto teardown;

            output_redirect_path = malloc((len + 1) * sizeof(char));
            memcpy(output_redirect_path, start, len);
        } else if (strncmp(start, "<", len) == 0) {
            free(input_redirect_path);

            start += len;
            if (start[0]) {
                start++;
            }
            len = parse_arg(start, cmd_len);
            ++i;
            if (len == 0)
                goto teardown;

            input_redirect_path = malloc((len + 1) * sizeof(char));
            memcpy(input_redirect_path, start, len);
        }
        start += len;
        if (start[0]) {
            start++;
        }
        ++i;
    }

    if (argc == 2 && strncmp(argv[0], "cd", 2) == 0) {
        chdir(argv[1]);
        goto teardown;
    }

    int pid = fork();

    if (pid == 0) {
        int ret;
        if (input_redirect_path) {
            puts("Attempting input redirect.");
            int fd = open(input_redirect_path);
            if (fd < 0)
                goto teardown;
            dup2(fd, 0);
        }
        if (output_redirect_path) {
            puts("Attempting output redirect. ");
            puts(output_redirect_path);
            putchar('\n');
            int fd = open(output_redirect_path);
            if (fd == -1)
                fd = creat(output_redirect_path);
            if (fd < 0)
                goto teardown;
            dup2(fd, 1);
        }
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

teardown:
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
