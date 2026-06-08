ARCH:=arm64

CC:=aarch64-none-elf-gcc
CXX:=aarch64-none-elf-g++
OBJCOPY:=aarch64-none-elf-objcopy

CXXFLAGS:=-std=c++20 -ffreestanding -nostdlib -fno-exceptions -mgeneral-regs-only -march=armv8-a+simd  -Wall -Wextra -Og -fno-inline -g
LDFLAGS:=-std=c++20 -ffreestanding -nostdlib -mgeneral-regs-only -lgcc -T link.ld
INCLUDES:=src/

TARGET:=build/myos

SRC_DIR:=src
OBJ_DIR:=build/obj
BIN_DIR:=build/bin

DIRS:=$(shell find src -type f)
SRCS:=$(wildcard $(SRC_DIR)/*.cc)
OBJS:=$(patsubst $(SRC_DIR)/%.cc, $(OBJ_DIR)/%.o, $(SRCS))

ASM_SRCS:=$(wildcard $(SRC_DIR)/*.s)
ASM_OBJS:=$(patsubst $(SRC_DIR)/%.s, $(OBJ_DIR)/%.o, $(ASM_SRCS))

.PHONY: all clean

all: $(TARGET)

$(TARGET): $(OBJS) $(ASM_OBJS) link.ld | $(BIN_DIR)
	$(CXX) $(LDFLAGS) $(OBJS) $(ASM_OBJS) -o $@

$(OBJ_DIR)/%.o: $(SRC_DIR)/%.cc | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) -I$(INCLUDES) -c $< -o $@

$(OBJ_DIR)/%.o: $(SRC_DIR)/%.s | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) -I$(INCLUDES) -c $< -o $@ 

$(OBJ_DIR) $(BIN_DIR):
	mkdir -p $@

clean:
	rm -rf build/

emulate: $(TARGET)
	qemu-system-aarch64 \
		-M virt -cpu cortex-a76 -smp 1 -m 1G \
		-display none \
		-kernel build/myos \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-device virtio-serial-device \
		-device virtconsole,chardev=ch0 \
		-chardev stdio,id=ch0,mux=on \
		-mon chardev=ch0,mode=readline \
		-serial null

debug: $(TARGET)
	qemu-system-aarch64 \
		-M virt -cpu cortex-a76 -smp 1 -m 1G \
		-display none \
		-kernel build/myos \
		-no-reboot \
		-global virtio-mmio.force-legacy=false \
		-device virtio-serial-device \
		-device virtconsole,chardev=ch0 \
		-chardev stdio,id=ch0,mux=on \
		-mon chardev=ch0,mode=readline \
		-serial null \
		-s -S

