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
debug: $(TARGET)
	qemu-system-aarch64 \
		-M virt,accel=hvf,gic-version=3 -cpu host -smp 1 -m 1G \
		-display none \
		-kernel $(OS_PATH) \
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
		-kernel $(OS_PATH) \
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
		-kernel $(OS_PATH) \
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
		-kernel $(OS_PATH) \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-device virtio-serial-device \
		-device virtconsole,chardev=ch0 \
		-chardev stdio,id=ch0,mux=on \
		-mon chardev=ch0,mode=readline \
		-serial chardev:ch0
endif

