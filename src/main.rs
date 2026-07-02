#![no_std]
#![no_main]

mod alloc;
mod arm_pl;
mod linker_symbols;
mod utils;

use core::{
    alloc::{GlobalAlloc, Layout}, arch::{asm, global_asm}, panic::PanicInfo,
};

use crate::{alloc::LLAllocator, utils::CoreLock};

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

    let allocator = CoreLock::new(LLAllocator::new(0xffffff80_48000000usize as _, 4096 * 10));
    let a = unsafe { allocator.alloc(Layout::new::<usize>()) };
    let b = unsafe { allocator.alloc(Layout::new::<u64>()) };
    let c = unsafe { allocator.alloc(Layout::new::<u32>()) };

    unsafe { allocator.dealloc(a, Layout::new::<usize>()) };


    panic!("It's joever");
}
