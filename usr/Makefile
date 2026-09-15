.PHONY: all

CC:=aarch64-none-elf-gcc
CFLAGS:=-nostdlib
LDFLAGS:=-nostdlib
LDLIBS:=-llibc

all: libc main cat ls

libc: lib.o
	aarch64-none-elf-ar rcs libc.a $^

main: main.o libc.a
	$(CC) $(LDFLAGS) $^ -o bin/main

cat: cat.o libc.a
	$(CC) $(LDFLAGS) $^ -o bin/cat

ls: ls.o libc.a
	$(CC) $(LDFLAGS) $^ -o bin/ls

clean:
	rm *.o *.a bin/*
