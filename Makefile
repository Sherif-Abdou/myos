HOST_OS:=$(shell uname -s)
TARGET:=build/myos
EXAMPLE_PROGRAM:=usr/main
OS_PATH:=target/aarch64-unknown-none-softfloat/debug/myos

.PHONY: all clean

all: $(TARGET)

$(TARGET):
	cargo b

$(EXAMPLE_PROGRAM): $(EXAMPLE_PROGRAM).c
	aarch64-none-elf-gcc -nostdlib $(EXAMPLE_PROGRAM).c -o $(EXAMPLE_PROGRAM)

clean:
	rm -rf build/


ifeq ($(HOST_OS),Darwin)
BASE_COMMAND:= qemu-system-aarch64 \
		-M virt,accel=hvf,gic-version=3 -cpu host -smp 1 -m 4G \
		-display none \
		-kernel $(OS_PATH) \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-chardev stdio,id=ch0,mux=on \
		-drive if=none,file=disk.qcow2,format=qcow2,id=hd0 \
		-device virtio-blk-device,drive=hd0 \
		-serial chardev:ch0
else
BASE_COMMAND:=qemu-system-aarch64 \
		-M virt,gic-version=3 -cpu cortex-a76 -smp 1 -m 4G \
		-display none \
		-kernel $(OS_PATH) \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-chardev stdio,id=ch0,mux=on \
		-drive if=none,file=disk.qcow2,format=qcow2,id=hd0 \
		-device virtio-blk-device,drive=hd0 \
		-serial chardev:ch0
endif

virt.dtb: $(TARGET)
	qemu-system-aarch64 \
		-M virt,gic-version=3,dumpdtb=virt.dtb -cpu cortex-a76 -smp 1 -m 4G \
		-display none \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-drive if=none,file=disk.qcow2,format=qcow2,id=hd0 \
		-device virtio-blk-device,drive=hd0 \

debug: $(EXAMPLE_PROGRAM) $(TARGET) virt.dtb
	cargo b
	$(BASE_COMMAND) -s -S
emulate: $(EXAMPLE_PROGRAM) $(TARGET) virt.dtb
	cargo b
	$(BASE_COMMAND)
