HOST_OS:=$(shell uname -s)
TARGET:=build/myos
OS_PATH:=target/aarch64-unknown-none-softfloat/debug/myos

.PHONY: all clean

all: $(TARGET)

$(TARGET):
	cargo b

clean:
	rm -rf build/


ifeq ($(HOST_OS),Darwin)
BASE_COMMAND:= qemu-system-aarch64 \
		-M virt,accel=hvf,gic-version=3 -cpu host -smp 1 -m 4G \
		-display none \
		-kernel $(OS_PATH) \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-device virtio-serial-device \
		-chardev stdio,id=ch0,mux=on \
		-serial chardev:ch0
else
BASE_COMMAND:=qemu-system-aarch64 \
		-M virt,gic-version=3 -cpu cortex-a76 -smp 1 -m 4G \
		-display none \
		-kernel $(OS_PATH) \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-device virtio-serial-device \
		-serial chardev:ch0
endif

virt.dtb: $(TARGET)
	qemu-system-aarch64 \
		-M virt,gic-version=3,dumpdtb=virt.dtb -cpu cortex-a76 -smp 1 -m 4G \
		-display none \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-device virtio-serial-device \
		-device virtconsole,chardev=ch0 \
		-chardev stdio,id=ch0,mux=on \
		-mon chardev=ch0,mode=readline

debug: $(TARGET) virt.dtb
	cargo b
	$(BASE_COMMAND) -s -S
emulate: $(TARGET) virt.dtb
	cargo b
	$(BASE_COMMAND)
