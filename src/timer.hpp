#pragma once

#include <cstdint>
class ArmTimer {
private:
    uint64_t counter_frequency;
public:
    static ArmTimer timer;

    void init();
    void enable();
    void disable();

    void set_frequency(uint64_t frequency);
    void set_delay(uint64_t microseconds);
};
