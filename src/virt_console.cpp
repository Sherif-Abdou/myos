#include "byte_alloc.hpp"
#include "utils.hpp"
#include <cstdint>
#include <virt_console.hpp>

void Console::init() {
    base[0x70/4] = 0x0;
    base[0x70/4] = 0x1;
    base[0x70/4] = 0x3;

    base[0x14/4] = 0;
    uint32_t supported_features = base[0x10];
    supported_features &= ~(1 << 1);

    base[0x24/4] = 0;
    base[0x20/4] = supported_features;

    base[0x70/4] = 0xb;
    base[0x70/4] = 0xf;
}

void Console::setup_rx_queue() {
    base[0x30/4] = 0;
    base[0x38/4] = 16;

    rx_desc_table = ByteAllocator::kalloc<std::array<VirtDescriptorTable, 16>>();
    rx_desc_avail = ByteAllocator::kalloc<VirtQueueAvailable<16>>();
    rx_desc_used = ByteAllocator::kalloc<VirtQueueUsed<16>>();

    base[0xb04] = virt_to_phys((uintptr_t)rx_desc_table) & 0xffffffff;
    base[0xb4/4] = virt_to_phys((uintptr_t)rx_desc_table) >> 32;
    base[0xc0/4] = virt_to_phys((uintptr_t)rx_desc_avail) & 0xffffffff;
    base[0xc4/4] = virt_to_phys((uintptr_t)rx_desc_avail) >> 32;
    base[0xd0/4] = virt_to_phys((uintptr_t)rx_desc_used) & 0xffffffff;
    base[0xd4/4] = virt_to_phys((uintptr_t)rx_desc_used) >> 32;

    base[0x44/4] = 1;
}

void Console::setup_tx_queue() {
    base[0x30/4] = 1;
    base[0x38/4] = 16;

    tx_desc_table = ByteAllocator::kalloc<std::array<VirtDescriptorTable, 16>>();
    tx_desc_avail = ByteAllocator::kalloc<VirtQueueAvailable<16>>();
    tx_desc_used = ByteAllocator::kalloc<VirtQueueUsed<16>>();

    base[0xb04] = virt_to_phys((uintptr_t)tx_desc_table) & 0xffffffff;
    base[0xb4/4] = virt_to_phys((uintptr_t)tx_desc_table) >> 32;
    base[0xc0/4] = virt_to_phys((uintptr_t)tx_desc_avail) & 0xffffffff;
    base[0xc4/4] = virt_to_phys((uintptr_t)tx_desc_avail) >> 32;
    base[0xd0/4] = virt_to_phys((uintptr_t)tx_desc_used) & 0xffffffff;
    base[0xd4/4] = virt_to_phys((uintptr_t)tx_desc_used) >> 32;

    base[0x44/4] = 1;
}
