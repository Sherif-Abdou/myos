#pragma once

#include <array>

class IsrManager {
  private:
    struct Isr {
        void (*isr)(void *);
        void *data;
    };
    static std::array<Isr, 1024> isrs;

  public:
    static void register_isr(int irqn, void (*isr)(void *),
                             void *data = nullptr);
    static void free_isr(int irqn);
    static void dispatch(int irqn);
};
