.global _exc_wrapper
.global _exc_entry

.global _sexc_wrapper
.global _sexc_entry

.macro save_regs
    sub sp, sp, #0x110
    stp x0, x1, [sp]
    stp x2, x3, [sp, #0x10]
    stp x4, x5, [sp, #0x20]
    stp x6, x7, [sp, #0x30]
    stp x8, x9, [sp, #0x40]
    stp x10, x11, [sp, #0x50]
    stp x12, x13, [sp, #0x60]
    stp x14, x15, [sp, #0x70]
    stp x16, x17, [sp, #0x80]
    stp x18, x19, [sp, #0x90]
    stp x20, x21, [sp, #0xa0]
    stp x22, x23, [sp, #0xb0]
    stp x24, x25, [sp, #0xc0]
    stp x26, x27, [sp, #0xd0]
    stp x28, x29, [sp, #0xe0]
    add x29, sp, #0x110
    stp x30, x29, [sp, #0xf0]
    mrs x0, elr_el1
    mrs x1, spsr_el1
    stp x0, x1, [sp, #0x100]
    mrs x0, sp_el0
    mov x1, #0
    stp x0, x1, [sp, #0x110]

    # Store pointer to register table
    mov x0, sp
.endm

# Not currently safe to be interrupted in this region.
.macro restore_regs
    ldp x8, x9, [x0, 0x110]
    msr sp_el0, x8

    ldp x8, x9, [x0, 0x100]
    msr elr_el1, x8
    msr spsr_el1, x9

    ldp x30, x29, [x0, 0xf0]
    mov sp, x29
    ldp x28, x29, [x0, 0xe0]
    ldp x26, x27, [x0, 0xd0]
    ldp x24, x25, [x0, 0xc0]
    ldp x22, x23, [x0, 0xb0]
    ldp x20, x21, [x0, 0xa0]
    ldp x18, x19, [x0, 0x90]
    ldp x16, x17, [x0, 0x80]
    ldp x14, x15, [x0, 0x70]
    ldp x12, x13, [x0, 0x60]
    ldp x10, x11, [x0, 0x50]
    ldp x8, x9, [x0, 0x40]
    ldp x6, x7, [x0, 0x30]
    ldp x4, x5, [x0, 0x20]
    ldp x2, x3, [x0, 0x10]
    ldp x0, x1, [x0]
.endm

_sexc_entry:
    b _sexc_wrapper

_sexc_wrapper:
    save_regs
    bl sexc_handler
    restore_regs

    eret

_exc_entry:
    b _exc_wrapper

_exc_wrapper:
    save_regs

    bl irq_handler

    restore_regs
    eret
