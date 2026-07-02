unsafe extern "C" {
    static __boot_start: u64;
    static __boot_end: u64;
    static __text_start: u64;
    static __text_end: u64;
    static __data_start: u64;
    static __data_end: u64;
    static __rodata_start: u64;
    static __rodata_end: u64;
    static __bss_start: u64;
    static __bss_end: u64;
    static __stack_start: u64;
    static __stack_end: u64;
    static __exc_vector: u64;
}
