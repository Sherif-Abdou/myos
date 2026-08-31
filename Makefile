HOST_OS:=$(shell uname -s)
TARGET:=build/myos
USER_MAIN:=usr/main
USER_LIB:=usr/lib.c
OS_PATH:=target/aarch64-unknown-none-softfloat/debug/myos

.PHONY: all clean

all: $(TARGET)

$(TARGET):
	cargo b

$(USER_MAIN): $(USER_MAIN).c
	aarch64-none-elf-gcc -nostdlib $(USER_LIB) $(USER_MAIN).c -o $(USER_MAIN)

clean:
	rm -rf build/


ifeq ($(HOST_OS),Darwin)
BASE_COMMAND:= qemu-system-aarch64 \
		-M virt,accel=hvf,gic-version=3 -cpu host -smp 2 -m 4G \
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
		-M virt,gic-version=3 -cpu cortex-a76 -smp 2 -m 4G \
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
		-M virt,gic-version=3,dumpdtb=virt.dtb -cpu cortex-a76 -smp 2 -m 4G \
		-display none \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-drive if=none,file=disk.qcow2,format=qcow2,id=hd0 \
		-device virtio-blk-device,drive=hd0 \

debug: $(USER_MAIN) $(TARGET) virt.dtb
	cargo b
	$(BASE_COMMAND) -s -S
emulate: $(USER_MAIN) $(TARGET) virt.dtb
	cargo b
	$(BASE_COMMAND)
