#pragma once

#include <optional>
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

template <size_t size> struct VirtQueueAvailable {
    uint16_t flags;
    uint16_t idx;
    uint16_t ring[size];
    uint16_t used;
};

template <size_t size> struct VirtQueueUsed {
    uint16_t flags;
    uint16_t idx;
    struct {
        uint16_t id;
        uint16_t len;
    } ring[size];
    uint16_t used;
};

class Console {
    constexpr static size_t queue_size = 16;
    volatile uint32_t *base;
    std::array<VirtDescriptorTable, queue_size> *rx_desc_table;
    VirtQueueAvailable<queue_size> *rx_desc_avail;
    VirtQueueUsed<queue_size> *rx_desc_used;

    std::array<VirtDescriptorTable, queue_size> *tx_desc_table;
    VirtQueueAvailable<queue_size> *tx_desc_avail;
    VirtQueueUsed<queue_size> *tx_desc_used;

    void setup_rx_queue();
    void setup_tx_queue();

    static std::optional<Console*> kernel_console;

    void init(volatile uint32_t *base_);

    void send_blocking(const char *buf, uint32_t len);
  public:
    static void create_console(volatile uint32_t *base_);

    static void print(const char *buf);
};
