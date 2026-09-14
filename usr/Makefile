.PHONY: all

CC:=aarch64-none-elf-gcc
CFLAGS:=-nostdlib
LDFLAGS:=-nostdlib
LDLIBS:=-llibc

all: libc main

libc: lib.o
	aarch64-none-elf-ar rcs libc.a $^

main: main.o libc.a
	$(CC) $(LDFLAGS) $^ -o main

clean:
	rm *.o *.a main
