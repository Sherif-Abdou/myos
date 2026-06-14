include src/irq/module.mk

vpath %.cc src/irq/

OBJS += byte_alloc.o exceptions.o main.o \
		page_alloc.o utils.o virt_console.o \
		bootstrap.o exception_entry.o \
		timer.o
