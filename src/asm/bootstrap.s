.extern entry

.section .data.bootstrap.pt, "a"
.global __dtb_addr
__dtb_addr:
    .quad 0xffffff8040000000

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
    ldr x2, =(0b010 << 32) | (0b10 << 30) | ((0b11001 << 16) | (0b11001) | (0b11 << 12) | (0b01) << 10 | (0b01) << 8)
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
    dc civac, x0
    dsb ish
    tlbi vmalle1is
    dsb ish
    isb sy
    msr sctlr_el1, x2
    isb sy
    ldr x1, =entry 
    ldr x4, =__stack_end
    mov sp, x4
    blr x1
    b _spin

_spin:
    b _spin

/*
.global __exc_vector
.balign 2048
__exc_vector:
.zero 512
b _sexc_entry
.zero 0x7C
b _exc_entry
.zero 0x7C
b _exc_entry
.zero 512
.zero 512
*/
