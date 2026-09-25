#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include <sys/_intsup.h>

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

void tokenizer_init(struct tokenizer *tokenizer, const char *str, size_t len) {
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

    tokenizer_init(&tokenizer, str, len);

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
    int arg_capacity;
    int argc;
    char **argv;
    int stdin_fd;
    int stdout_fd;
    int pid;
};

struct pipeline {
    int process_count;
    int process_capacity;
    struct pipeline_process *processes;
    const char *input_redirect_path;
    const char *output_redirect_path;
};

void pipeline_init(struct pipeline *pipeline) {
    memset(pipeline, 0, sizeof(struct pipeline));
    pipeline->processes = malloc(2 * sizeof(struct pipeline_process));
    pipeline->process_capacity = 2;
}

void pipeline_process_free(struct pipeline_process *pipeline_process) {
    if (pipeline_process->stdin_fd >= 0) {
        close(pipeline_process->stdin_fd);
    }
    if (pipeline_process->stdout_fd >= 0) {
        close(pipeline_process->stdout_fd);
    }
    for (int i = 0; i < pipeline_process->argc; ++i) {
        free(pipeline_process->argv[i]);
    }
    free(pipeline_process->argv);
}

void pipeline_free(struct pipeline *pipeline) {
    for (int i = 0; i < pipeline->process_count; ++i)
        pipeline_process_free(&pipeline->processes[i]);

    free(pipeline->processes);
};

void pipeline_process_init(struct pipeline_process *process) {
    process->argc = 0;
    process->arg_capacity = 8;
    process->argv = malloc(8 * sizeof(char *));
    process->stdin_fd = -1;
    process->stdout_fd = -1;
    process->pid = -1;
    memset(process->argv, 0, sizeof(char *) * 8);
}

void pipeline_process_double_capacity(struct pipeline_process *process) {
    void *argv = malloc(sizeof(char *) * (process->arg_capacity * 2));

    memcpy(argv, process->argv, process->argc * sizeof(char *));

    free(process->argv);

    process->argv = argv;
    process->arg_capacity *= 2;
}

void pipeline_double_capacity(struct pipeline *pipeline) {
    void *processes = malloc(sizeof(struct pipeline_process) *
                             (pipeline->process_capacity * 2));

    memcpy(processes, pipeline->processes,
           pipeline->process_capacity * sizeof(struct pipeline_process));

    free(pipeline->processes);

    pipeline->processes = processes;
    pipeline->process_capacity *= 2;
}

void parse_process(struct tokenizer *tokenizer, struct token *token,
                   struct pipeline *pipeline) {
    struct pipeline_process *process = NULL;

    while (1) {
        tokenizer_next_token(tokenizer, token);
        if (token->len == 0)
            break;
        if ((strncmp(token->str, ">", token->len) == 0) ||
            (strncmp(token->str, "<", token->len) == 0)) {
            break;
        }
        if (strncmp(token->str, "|", token->len) == 0)
            break;
        if (pipeline->process_capacity == pipeline->process_count)
            pipeline_double_capacity(pipeline);

        if (!process) {
            process = &pipeline->processes[pipeline->process_count];
            pipeline_process_init(process);
            if (pipeline->process_count > 0) {
                int fds[2];
                pipe(fds);

                pipeline->processes[pipeline->process_count - 1].stdout_fd =
                    fds[1];
                pipeline->processes[pipeline->process_count].stdin_fd = fds[0];
            }

            pipeline->process_count++;
        }

        if (process->argc == process->arg_capacity)
            pipeline_process_double_capacity(process);

        process->argv[process->argc] = malloc((token->len + 1) * sizeof(char));
        memcpy(process->argv[process->argc], token->str, token->len);
        process->argv[process->argc][token->len] = 0;
        process->argc++;
    }
}

void parse_command(const char *cmd, size_t cmd_len) {
    int i;

    int arg_count = count_tokens(cmd, cmd_len);

    if (arg_count == 0) {
        return;
    }

    struct token token;
    struct tokenizer tokenizer;

    struct pipeline pipeline;
    tokenizer_init(&tokenizer, cmd, cmd_len);
    pipeline_init(&pipeline);

    while (1) {
        parse_process(&tokenizer, &token, &pipeline);
        if (token.len == 0)
            break;
        if (strncmp(token.str, "|", token.len) == 0)
            continue;
        break;
    }
    char *output_redirect_path = NULL;
    char *input_redirect_path = NULL;

    do {
        if (token.len == 0)
            break;

        if (strncmp(token.str, ">", token.len) == 0) {
            free(output_redirect_path);

            tokenizer_next_token(&tokenizer, &token);

            if (token.len == 0)
                goto teardown;

            output_redirect_path = malloc((token.len + 1) * sizeof(char));
            memcpy(output_redirect_path, token.str, token.len);
            tokenizer_next_token(&tokenizer, &token);
        } else if (strncmp(token.str, "<", token.len) == 0) {
            free(input_redirect_path);

            tokenizer_next_token(&tokenizer, &token);

            if (token.len == 0)
                goto teardown;

            input_redirect_path = malloc((token.len + 1) * sizeof(char));
            memcpy(input_redirect_path, token.str, token.len);
            tokenizer_next_token(&tokenizer, &token);
        } else {
            break;
        }
    } while (token.len > 0);


    for (int process_index = 0; process_index < pipeline.process_count;
         process_index++) {
        struct pipeline_process *process = &pipeline.processes[process_index];
        if (process->argc == 2 && strncmp(process->argv[0], "cd", 2) == 0) {
            chdir(process->argv[1]);
            continue;
        }
        process->pid = fork();

        if (process->pid == 0) {
            int ret;
            if (process->stdin_fd >= 0) {
                dup2(process->stdin_fd, 0);
            } else if (input_redirect_path) {
                int fd = open(input_redirect_path);
                if (fd < 0)
                    goto teardown;
                dup2(fd, 0);
            }

            if (process->stdout_fd >= 0) {
                dup2(process->stdout_fd, 1);
            } else if (output_redirect_path) {
                int fd = open(output_redirect_path);
                if (fd == -1)
                    fd = creat(output_redirect_path);
                if (fd < 0)
                    goto teardown;
                dup2(fd, 1);
            }
            ret = exec(process->argv[0], process->argc,
                       (const char **)process->argv);
            if (ret < 0) {
                char *new_path =
                    malloc(strlen(process->argv[0]) + strlen("/bin/") - 1);
                memcpy(new_path, "/bin/", sizeof("/bin/"));
                memcpy(new_path + sizeof("/bin/") - 1, process->argv[0],
                       strlen(process->argv[0]) + 1);
                free(process->argv[0]);
                process->argv[0] = new_path;
                ret = exec(process->argv[0], process->argc,
                           (const char **)process->argv);
            }
        } else {
            if (process->stdin_fd >= 0) {
                close(process->stdin_fd);
                process->stdin_fd = -1;
            }

            if (process->stdout_fd >= 0) {
                close(process->stdout_fd);
                process->stdout_fd = -1;
            }
        }
    }

    for (int process_index = 0; process_index < pipeline.process_count;
         process_index++) {
        waitpid(pipeline.processes[process_index].pid);
    }

teardown:
    pipeline_free(&pipeline);
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
