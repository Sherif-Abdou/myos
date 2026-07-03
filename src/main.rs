#![no_std]
#![no_main]
#![feature(allocator_api)]

mod allocators;
mod arm_pl;
mod dtb;
mod linker_symbols;
mod utils;

extern crate alloc;

use core::{
    arch::{asm, global_asm},
    panic::PanicInfo,
    ptr::NonNull,
};

use crate::{
    allocators::BumpAllocator, dtb::Fdt, utils::{CoreLock, LazyCoreLock},
};

global_asm!(include_str!("asm/bootstrap.s"));

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(message) = info.message().as_str() {
        early_printk!("{}\n", message);
    }

    loop {
        unsafe { asm!("wfe") };
    }
}

#[unsafe(no_mangle)]
extern "C" fn entry() {
    early_printk!("Hello kernel\n");

    let _fdt = Fdt::from_boot();

    panic!("It's joever");
}
