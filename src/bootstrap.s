.extern loop

.section .data.bootstrap.pt, "a"
.balign 4096
__initial_pt1:
    .quad 0x00000401
    .quad 0x40000401
    .quad 0x0

.section .text.bootstrap, "a"
.global _start

_start:
    b __configure_pt

__configure_pt:
    ldr x2, =((0b11001 << 16) | (0b11001) | (0b11 << 12) | (0b01) << 10 | (0b01) << 8)
    msr tcr_el1, x2
    adr x2, __initial_pt1
    b __enable_mmu
    
# x2 is the top level base register
__enable_mmu:
    ldr x0, =0x00000000000004FF
    msr mair_el1, x0
    msr ttbr1_el1, x2
    msr ttbr0_el1, x2
    isb sy
    mrs x2, sctlr_el1
    orr x2, x2, #1
    msr sctlr_el1, x2
    isb sy
    ldr x1, =loop 
    ldr x4, =__sstack
    mov sp, x4
    blr x1
    b _spin

_spin:
    b _spin

.global __exc_vector
.balign 2048
__exc_vector:
.zero 512
b _exc_entry
.zero 76
b _exc_entry
.zero 76
b _exc_entry
.zero 512
.zero 512
