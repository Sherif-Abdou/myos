#![no_std]
#![no_main]
#![feature(allocator_api)]

mod allocators;
mod arm_pl;
mod dtb;
mod linker_symbols;
mod memory;
mod utils;

extern crate alloc;

use core::{
    arch::{asm, global_asm},
    panic::PanicInfo,
};

use alloc::boxed::Box;

use crate::{
    allocators::LLAllocator,
    arm_pl::init_from_dtb_node,
    dtb::{Fdt, find_earlyconsole_node},
    memory::{PAGE_ALLOCATOR, init_allocator},
    utils::{SpinLock, cpu_id},
};

global_asm!(include_str!("asm/bootstrap.s"));

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_printk!("PANIC\n");
    if let Some(message) = info.message().as_str() {
        early_printk!("{}\n", message);
    }

    loop {
        unsafe { asm!("wfe") };
    }
}

static BIG_ALLOCATOR: SpinLock<LLAllocator> = SpinLock::new(LLAllocator::new());

#[unsafe(no_mangle)]
extern "C" fn entry() {
    let fdt = Fdt::from_boot();
    let earlyconsole_node = find_earlyconsole_node(&fdt);
    init_from_dtb_node(earlyconsole_node);

    let root = fdt.nodes().next().unwrap();

    let memory = root
        .children()
        .find(|node| node.name().starts_with("memory"))
        .unwrap();

    init_allocator(memory);

    let pages = PAGE_ALLOCATOR.lock().reserve_pages(24);

    let data = Box::<[u8; 5000], &'static SpinLock<LLAllocator>>::new_uninit_in(&BIG_ALLOCATOR);

    early_printk!("Heap pages: {:?}\n", pages);
    early_printk!("Done.\n");
    loop {
        unsafe { asm!("wfe") };
    }
}
