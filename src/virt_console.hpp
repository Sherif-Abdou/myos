#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

struct VirtDescriptorTable {
    // Physical address of queue.
    uint64_t addr;
    // Length of the queue.
    uint32_t len;
    // Flags
    uint16_t flags;
    // Next descriptor index
    uint16_t next;
};

template<size_t size>
struct VirtQueueAvailable {
    uint16_t flags;
    uint16_t idx;
    uint16_t ring[size];
    uint16_t used;
};

template<size_t size>
struct VirtQueueUsed {
    uint16_t flags;
    uint16_t idx;
    struct {
        uint16_t id;
        uint16_t len;
    } ring[size];
    uint16_t used;
};

class Console {
    volatile uint32_t *base;
    std::array<VirtDescriptorTable, 16> *rx_desc_table;
    VirtQueueAvailable<16> *rx_desc_avail;
    VirtQueueUsed<16> *rx_desc_used;

    std::array<VirtDescriptorTable, 16> *tx_desc_table;
    VirtQueueAvailable<16> *tx_desc_avail;
    VirtQueueUsed<16> *tx_desc_used;

    void setup_rx_queue();
    void setup_tx_queue();

public:
    void init(volatile uint32_t *base_);

    void send_blocking(const char *buf, uint32_t len);
};
