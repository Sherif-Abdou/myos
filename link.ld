ENTRY(_start)

MEMORY {
    KERNEL (RWX) : ORIGIN = 0xFFFFFF8040080000, LENGTH = 0xFFFFFFFF
    PHYSICAL (RWX) : ORIGIN = 0x40080000, LENGTH = 0xFFFFFFF
}

SECTIONS
{

    .boot : {
        __boot_start = .;
        *(.text.bootstrap .text.bootstrap.*);
        *(.data.bootstrap .data.bootstrap.*);
        __boot_end = .;
    } > KERNEL AT> PHYSICAL


    .text : {
        __text_start = .;
        *(.text .text.*);
        __text_end = .;
    } > KERNEL AT> PHYSICAL

    .data : {
        __data_start = .;
        *(.data .data.*);
        __data_end = .;
    } > KERNEL AT> PHYSICAL

    .bss (NOLOAD) : {
        __bss_start = .;
        *(.bss .bss.*);
        __bss_end = .;
    } > KERNEL

      .stack (NOLOAD) : ALIGN(16)
      {
        __estack = .;
        . += ALIGN(65536, 16);
        __sstack = .;
      } > KERNEL
}
