include src/module.mk

HOST_OS:=$(shell uname -s)
ARCH:=arm64

CC:=aarch64-none-elf-gcc
CXX:=aarch64-none-elf-g++
OBJCOPY:=aarch64-none-elf-objcopy

DEPFLAGS = -MMD -MP

CXXFLAGS+=-std=c++20
CXXFLAGS+=-ffreestanding -nostdlib -fno-exceptions -mgeneral-regs-only
CXXFLAGS+=-march=armv8-a+simd -Wall -Wextra $(DEPFLAGS)
CXXFLAGS+=-Og -fno-inline -g

LDFLAGS:=-std=c++20 -ffreestanding -nostdlib -mgeneral-regs-only -T link.ld
INCLUDES:=src/

OBJDIR:=build
TARGET:=build/myos

.PHONY: all clean
vpath %.cc src/
vpath %.s src/

OBJDIRS:=$(addprefix $(OBJDIR)/, $(sort $(dir $(OBJS))))
-include $(OBJS:.o=.d)

all: $(TARGET)

$(TARGET): $(OBJS) link.ld | $(OBJDIRS)
	$(CXX) $(CXXFLAGS) $(LDFLAGS) $(addprefix $(OBJDIR)/, $(OBJS)) -o $@ -lgcc

%.o: %.cc | $(OBJDIRS)
	$(CXX) $(CXXFLAGS) -I$(INCLUDES) -c $< -o build/$@ -lgcc

%.o: %.s | $(OBJDIRS)
	$(CXX) $(CXXFLAGS) -I$(INCLUDES) -c $< -o build/$@  -lgcc

$(OBJDIRS):
	mkdir -p $@

clean:
	rm -rf build/


ifeq ($(HOST_OS),Darwin)
debug: $(TARGET)
	qemu-system-aarch64 \
		-M virt,accel=hvf,gic-version=3 -cpu host -smp 1 -m 1G \
		-display none \
		-kernel build/myos \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-device virtio-serial-device \
		-device virtconsole,chardev=ch0 \
		-chardev stdio,id=ch0,mux=on \
		-mon chardev=ch0,mode=readline \
		-serial chardev:ch0 \
		-s -S
emulate: $(TARGET)
	qemu-system-aarch64 \
		-M virt,accel=hvf,gic-version=3 -cpu host -smp 1 -m 1G \
		-display none \
		-kernel build/myos \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-device virtio-serial-device \
		-device virtconsole,chardev=ch0 \
		-chardev stdio,id=ch0,mux=on \
		-mon chardev=ch0,mode=readline \
		-serial chardev:ch0
else
debug: $(TARGET)
	qemu-system-aarch64 \
		-M virt,gic-version=3 -cpu cortex-a76 -smp 1 -m 1G \
		-display none \
		-kernel build/myos \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-device virtio-serial-device \
		-device virtconsole,chardev=ch0 \
		-chardev stdio,id=ch0,mux=on \
		-mon chardev=ch0,mode=readline \
		-serial chardev:ch0 \
		-s -S
emulate: $(TARGET)
	qemu-system-aarch64 \
		-M virt,gic-version=3 -cpu cortex-a76 -smp 1 -m 1G \
		-display none \
		-kernel build/myos \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-device virtio-serial-device \
		-device virtconsole,chardev=ch0 \
		-chardev stdio,id=ch0,mux=on \
		-mon chardev=ch0,mode=readline \
		-serial chardev:ch0
endif

