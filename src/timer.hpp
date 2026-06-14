#pragma once

#include <cstdint>
class ArmTimer {
private:
    uint64_t counter_frequency;
public:
    static ArmTimer timer;

    void init();
    void enable();

    void set_frequency(uint64_t frequency);
};
