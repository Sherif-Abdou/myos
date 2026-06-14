#include "byte_alloc.hpp"
#include "utils.hpp"
#include <cstdint>
#include <optional>
#include <virt_console.hpp>

std::optional<Console *> Console::kernel_console{std::nullopt};

void Console::init(volatile uint32_t *base_) {
    base = base_;

    base[0x70 / 4] = 0x0;
    base[0x70 / 4] = 0x1;
    base[0x70 / 4] = 0x3;

    base[0x14 / 4] = 0;
    uint32_t supported_features = base[0x10 / 4];
    supported_features &= ~(1 << 1);

    base[0x24 / 4] = 0;
    base[0x20 / 4] = supported_features;

    base[0x70 / 4] = 0xb;

    setup_rx_queue();
    setup_tx_queue();

    base[0x070 / 4] = 0xf;
}

void Console::setup_rx_queue() {
    base[0x30 / 4] = 0;
    base[0x38 / 4] = 16;

    rx_desc_table =
        ByteAllocator::kalloc<std::array<VirtDescriptorTable, 16>>();
    rx_desc_avail = ByteAllocator::kalloc<VirtQueueAvailable<16>>();
    rx_desc_used = ByteAllocator::kalloc<VirtQueueUsed<16>>();

    base[0x80 / 4] = virt_to_phys((uintptr_t)rx_desc_table) & 0xffffffff;
    base[0x84 / 4] = virt_to_phys((uintptr_t)rx_desc_table) >> 32;
    base[0x90 / 4] = virt_to_phys((uintptr_t)rx_desc_avail) & 0xffffffff;
    base[0x94 / 4] = virt_to_phys((uintptr_t)rx_desc_avail) >> 32;
    base[0xa0 / 4] = virt_to_phys((uintptr_t)rx_desc_used) & 0xffffffff;
    base[0xa4 / 4] = virt_to_phys((uintptr_t)rx_desc_used) >> 32;

    base[0x44 / 4] = 1;
}

void Console::setup_tx_queue() {
    base[0x30 / 4] = 1;
    base[0x38 / 4] = 16;

    tx_desc_table =
        ByteAllocator::kalloc<std::array<VirtDescriptorTable, 16>>();
    tx_desc_avail = ByteAllocator::kalloc<VirtQueueAvailable<16>>();
    tx_desc_used = ByteAllocator::kalloc<VirtQueueUsed<16>>();

    base[0x80 / 4] = virt_to_phys((uintptr_t)tx_desc_table) & 0xffffffff;
    base[0x84 / 4] = virt_to_phys((uintptr_t)tx_desc_table) >> 32;
    base[0x90 / 4] = virt_to_phys((uintptr_t)tx_desc_avail) & 0xffffffff;
    base[0x94 / 4] = virt_to_phys((uintptr_t)tx_desc_avail) >> 32;
    base[0xa0 / 4] = virt_to_phys((uintptr_t)tx_desc_used) & 0xffffffff;
    base[0xa4 / 4] = virt_to_phys((uintptr_t)tx_desc_used) >> 32;

    base[0x44 / 4] = 1;
}

void Console::send_blocking(const char *buf, uint32_t len) {
    uint16_t idx = 0;
    (*tx_desc_table)[idx] = {
        .addr = virt_to_phys((size_t)buf),
        .len = len,
        .flags = 0,
        .next = 0,
    };

    uint32_t avail_slot = tx_desc_avail->idx;
    tx_desc_avail->ring[avail_slot] = idx;

    uint16_t last_used_idx = tx_desc_used->idx;

    asm volatile("dmb ish" ::: "memory");
    tx_desc_avail->idx++;
    asm volatile("dmb ish" ::: "memory");


    base[0x50 / 4] = 1;

    // Busyloop until done
    while (last_used_idx == tx_desc_used->idx) {
        asm volatile("nop");
    }
}

void Console::create_console(volatile uint32_t *base_) {
    kernel_console.emplace(ByteAllocator::kalloc<Console>());
    kernel_console.value()->init(base_);
}

void Console::print(const char *buf) {
    if (kernel_console.has_value()) {
        kernel_console.value()->send_blocking(buf, strlen(buf));
    }
}
