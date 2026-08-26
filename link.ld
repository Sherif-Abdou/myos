ENTRY(_start)

MEMORY {
    KERNEL (RWX) : ORIGIN = 0xFFFFFF8040100000, LENGTH = 0xFFFFFFFF
    PHYSICAL (RWX) : ORIGIN = 0x40100000, LENGTH = 0xFFFFFFF
}

SECTIONS
{
    .boot : {
        __boot_start = .;
        *(.text.bootstrap .text.bootstrap.*);
        *(.data.bootstrap .data.bootstrap.*);
        __boot_end = .;
    } > KERNEL AT> PHYSICAL
    __kernel_start = ADDR(.boot);

    .text : ALIGN(4096) {
        __text_start = .;
        *(.text .text.*);
        __text_end = .;
    } > KERNEL AT> PHYSICAL

    .data : ALIGN(4096) {
        __data_start = .;
        *(.data .data.*);
        __data_end = .;
    } > KERNEL AT> PHYSICAL

    .rodata : ALIGN(4096) {
        __rodata_start = .;
        *(.rodata .rodata.*);
        __rodata_end = .;
    } > KERNEL AT> PHYSICAL
    __kernel_end = .;

    .bss (NOLOAD) : ALIGN(4096) {
        __bss_start = .;
        *(.bss .bss.*);
        __bss_end = .;
    } > KERNEL

      .stack (NOLOAD) : ALIGN(4096)
      {
        __stack_start = .;
        . += ALIGN(65536, 16);
        __stack_end = .;
      } > KERNEL

      .heap (NOLOAD) : ALIGN(4096)
      {
        __heap_start = .;
        . += ALIGN(65536, 16);
        __heap_end = .;
      } > KERNEL

      __kernel_end = .;

    /* /DISCARD/ : { */
    /*     *(.eh_frame) */
    /*     *(.eh_frame_hdr) */
    /* } */
}

__kernel_size = __kernel_end - __kernel_start;
