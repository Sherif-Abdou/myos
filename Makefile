ARCH:=arm64

CC:=aarch64-none-elf-gcc
CXX:=aarch64-none-elf-g++
OBJCOPY:=aarch64-none-elf-objcopy

CXXFLAGS:=-Wall -Wextra -ffreestanding -fno-exceptions -march=armv8-a+simd -nostdlib -g
LDFLAGS:=-T link.ld
INCLUDES:=include/

TARGET:=build/myos

SRC_DIR:=src
OBJ_DIR:=build/obj
BIN_DIR:=build/bin

SRCS:=$(wildcard $(SRC_DIR)/*.cc)
OBJS:=$(patsubst $(SRC_DIR)/%.cc, $(OBJ_DIR)/%.o, $(SRCS))

ASM_SRCS:=$(wildcard $(SRC_DIR)/*.s)
ASM_OBJS:=$(patsubst $(SRC_DIR)/%.s, $(OBJ_DIR)/%.o, $(ASM_SRCS))

.PHONY: all clean

all: $(TARGET)

$(TARGET): $(OBJS) $(ASM_OBJS) link.ld | $(BIN_DIR)
	$(CXX) $(CXXFLAGS) $(LDFLAGS) $(OBJS) $(ASM_OBJS) -o $@

$(OBJ_DIR)/%.o: $(SRC_DIR)/%.cc | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) -I$(INCLUDES) -c $< -o $@

$(OBJ_DIR)/%.o: $(SRC_DIR)/%.s | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) -I$(INCLUDES) -c $< -o $@

$(OBJ_DIR) $(BIN_DIR):
	mkdir -p $@

clean:
	rm -rf build/

emulate: $(TARGET)
	qemu-system-aarch64 -M virt -cpu cortex-a76 -smp 1 -m 128M -nographic -kernel build/myos

debug: $(TARGET)
	qemu-system-aarch64 -M virt -cpu cortex-a76 -smp 1 -m 128M -nographic -kernel build/myos -s -S
