.PHONY: all

CC:=aarch64-none-elf-gcc
CFLAGS:=-nostdlib
LDFLAGS:=-nostdlib
LDLIBS:=-llibc

all: libc main cat ls rm rmdir

libc: lib.o
	aarch64-none-elf-ar rcs libc.a $^

main: main.o libc.a
	$(CC) $(LDFLAGS) $^ -o bin/main

cat: cat.o libc.a
	$(CC) $(LDFLAGS) $^ -o bin/cat

ls: ls.o libc.a
	$(CC) $(LDFLAGS) $^ -o bin/ls

rm: rm.o libc.a
	$(CC) $(LDFLAGS) $^ -o bin/rm

rmdir: rmdir.o libc.a
	$(CC) $(LDFLAGS) $^ -o bin/rmdir

clean:
	rm *.o *.a bin/*
