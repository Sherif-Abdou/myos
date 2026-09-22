#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>

#include "lib.h"

const char *base = "hello world\n";
static volatile int a;

#define MAX_CMD_LEN 16

struct token {
    const char *str;
    size_t len;
};

struct tokenizer {
    const char *str;
    size_t remaining_len;
};

void init_tokenizer(struct tokenizer *tokenizer, const char *str, size_t len) {
    tokenizer->str = str;
    tokenizer->remaining_len = len;
}

int tokenizer_next_token(struct tokenizer *tokenizer, struct token *token) {
    int i = 0;

    // Skip whitespace.
    while (tokenizer->str[0] == ' ' && tokenizer->remaining_len > 0) {
        tokenizer->str++;
        tokenizer->remaining_len--;
    };

    while (i < tokenizer->remaining_len && tokenizer->str[i]) {
        if (tokenizer->str[i] == ' ') {
            goto output;
        }
        ++i;
    }

output:
    token->str = tokenizer->str;
    token->len = i;
    tokenizer->str += i;
    tokenizer->remaining_len -= i;
    return i;
}

int count_tokens(const char *str, size_t len) {
    struct tokenizer tokenizer;
    struct token token;

    init_tokenizer(&tokenizer, str, len);

    int i = 0;
    do {
        tokenizer_next_token(&tokenizer, &token);
        if (token.len > 0)
            ++i;
    } while (token.len > 0);

    return i;
}

ssize_t parse_arg(const char *line, size_t len) {
    int i = 0;
    while (i < len && line[i]) {
        if (line[i] == ' ')
            return i;

        ++i;
    }

    return i;
}

struct pipeline_process {
    int argc;
    const char *argv;
};

struct pipeline {
    int process_count;
    struct pipeline_process *processes;
    const char *input_redirect_path;
    const char *output_redirect_path;
};

void parse_command(const char *cmd, size_t cmd_len) {
    int i;

    int arg_count = count_tokens(cmd, cmd_len);

    if (arg_count == 0) {
        return;
    }

    int argc = 0;

    char **argv = malloc(sizeof(char *) * arg_count);

    struct token token;
    struct tokenizer tokenizer;
    init_tokenizer(&tokenizer, cmd, cmd_len);

    for (i = 0; i < arg_count; ++i) {
        tokenizer_next_token(&tokenizer, &token);
        if ((strncmp(token.str, ">", token.len) == 0) ||
            (strncmp(token.str, "<", token.len) == 0)) {
            break;
        }
        argv[i] = malloc((token.len + 1) * sizeof(char));
        memcpy(argv[i], token.str, token.len);
        argv[i][token.len] = 0;
        argc++;
    }
    char *output_redirect_path = NULL;
    char *input_redirect_path = NULL;

    // Redirections spotted
    while (i < arg_count) {
        tokenizer_next_token(&tokenizer, &token);

        if (strncmp(token.str, ">", token.len) == 0) {
            free(output_redirect_path);

            tokenizer_next_token(&tokenizer, &token);

            ++i;
            if (token.len == 0)
                goto teardown;

            output_redirect_path = malloc((token.len + 1) * sizeof(char));
            memcpy(output_redirect_path, token.str, token.len);
        } else if (strncmp(token.str, "<", token.len) == 0) {
            free(input_redirect_path);

            tokenizer_next_token(&tokenizer, &token);
            ++i;
            if (token.len == 0)
                goto teardown;

            input_redirect_path = malloc((token.len + 1) * sizeof(char));
            memcpy(input_redirect_path, token.str, token.len);
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
            int fd = open(input_redirect_path);
            if (fd < 0)
                goto teardown;
            dup2(fd, 0);
        }
        if (output_redirect_path) {
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
