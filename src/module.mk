include src/irq/module.mk
include src/print/module.mk

vpath %.cc src/irq/
vpath %.cc src/print/

OBJS += byte_alloc.o exceptions.o main.o \
		page_alloc.o utils.o virt_console.o \
		bootstrap.o exception_entry.o \
		timer.o
