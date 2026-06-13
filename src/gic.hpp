#pragma once

#include "utils.hpp"
#include <cstdint>
#include <stddef.h>

/* Distributor Registers. */

// Distributor control register.
static constexpr size_t GICD_CTRL = 0;
// Interrupt controller type register.
static constexpr size_t GICD_TYPER = 0x4;
// Distributor Implementer Identification Register.
static constexpr size_t GICD_IIDR = 0x8;
// Interrupt Controller Type 2 Register. 
static constexpr size_t GICD_TYPER2 = 0xC;
// Error Reporting Status Register (optional).
static constexpr size_t GICD_STATUSR = 0x10;
// Set SPI Register.
static constexpr size_t GICD_SETSPI_NSR = 0x40;
// Clear SPI Register.
static constexpr size_t GICD_CLRSPI_NSR = 0x48;
// Interrupt Group Registers.
static constexpr size_t GICD_IGROUPR(size_t n) { return 0x100 + 0x4 * n; }
// Interrupt Set-Enable Registers.
static constexpr size_t GICD_ICENABLER(size_t n) { return 0x180 + 0x4 * n; }
// Interrupt Set-Pending Registers.
static constexpr size_t GICD_ISPENDR(size_t n) { return 0x200 + 0x4 * n; }
// Interrupt Clear-Pending Registers.
static constexpr size_t GICD_ICPENDR(size_t n) { return 0x280 + 0x4 * n; }
// Interrupt Set-Active Registers.
static constexpr size_t GICD_ISACTIVER(size_t n) { return 0x300 + 0x4 * n; }
// Interrupt Clear-Active Registers.
static constexpr size_t GICD_ICACTIVER(size_t n) { return 0x380 + 0x4 * n; }
// Interrupt Priority Registers.
static constexpr size_t GICD_IPRIORITY(size_t n) { return 0x400 + 0x4 * n; }
// Interrupt Processor Target Registers.
static constexpr size_t GICD_ITARGETSR(size_t n) { return 0x800 + 0x4 * n; }
// Interrupt Configuration Registers.
static constexpr size_t GICD_ICFGR(size_t n) { return 0xC00 + 0x4 * n; }
// Non-secure Access Registers.
static constexpr size_t GICD_NSACR(size_t n) { return 0xE00 + 0x4 * n; }
// Software Generation Interrupt Registers.
static constexpr size_t GICD_SGIR = 0xF00;
// Interrupt Routing Registers.
static constexpr size_t GICD_IROUTER(size_t n) { return 0x6100 + 0x4 * n; }

/* Redistributor Registers. */
// Redistributor Control Register.
static constexpr size_t GICR_CTLR = 0x0;
// Implementer Identification Register.
static constexpr size_t GICR_IIDR = 0x4;
// Redistributor Type Register.
static constexpr size_t GICR_TYPER = 0x8;
// Error Reporting Status Register, optional.
static constexpr size_t GICR_STATUSR = 0x10;
// Redistributor Wake Register.
static constexpr size_t GICR_WAKER = 0x14;
// Report maximum PARTID and PMG Register.
static constexpr size_t GICR_MPAMIDR = 0x18;
// Set PARTID and PMG Register.
static constexpr size_t GICR_PARTIDR = 0x1C;
// Redistributor Properties Base Address Register.
static constexpr size_t GICR_PROPBASER = 0x70;
// Redistributor LPI Pending Tabler Base Address Register.
static constexpr size_t GICR_PENDBASER = 0X78;

// Interrupt Group Register 0.
static constexpr size_t GICR_IGROUPR0 = 0x80;
// Interrupt Set-Enable Register 0.
static constexpr size_t GICR_ISENABLER0 = 0x100;
// Interrupt Clear-Enable Register 0.
static constexpr size_t GICR_ICENABLER0 = 0x180;
// Interrupt Set-Pending Register 0.
static constexpr size_t GICR_ISPENDR0 = 0x200;
// Interrupt Clear-Pending Register 0.
static constexpr size_t GICR_ICPENDR0 = 0x280;
// Interrupt Set-Active Register 0.
static constexpr size_t GICR_ISACTIVER0 = 0x200;
// Interrupt Clear-Active Register 0.
static constexpr size_t GICR_ICACTIVER0 = 0x280;

class VolatileRegion {
  private:
    volatile uint8_t *base;

  public:
    VolatileRegion(volatile uint8_t *base) : base(base) {}

    uint8_t readb(size_t offset = 0) {
        return *reinterpret_cast<volatile uint8_t *>(base + offset);
    }

    uint16_t readh(size_t offset = 0) {
        return *reinterpret_cast<volatile uint16_t *>(base + offset);
    }

    uint32_t readw(size_t offset = 0) {
        return *reinterpret_cast<volatile uint32_t *>(base + offset);
    }

    uint64_t readl(size_t offset = 0) {
        return *reinterpret_cast<volatile uint64_t *>(base + offset);
    }

    void writeb(uint8_t value, size_t offset = 0) {
        *reinterpret_cast<volatile uint8_t *>(base + offset) = value;
    }

    void writeh(uint16_t value, size_t offset = 0) {
        *reinterpret_cast<volatile uint16_t *>(base + offset) = value;
    }

    void writew(uint32_t value, size_t offset = 0) {
        *reinterpret_cast<volatile uint32_t *>(base + offset) = value;
    }

    void writel(uint64_t value, size_t offset = 0) {
        *reinterpret_cast<volatile uint64_t *>(base + offset) = value;
    }

    VolatileRegion region_at_offset(size_t offset) {
        return VolatileRegion(base + offset);
    }
};

class Gic {
    VolatileRegion distributor;

    void init_distributor();
    void enable_system_registers();
  public:
    void init();
};
