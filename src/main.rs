#![no_std]
#![no_main]
#![feature(allocator_api)]
#![feature(maybe_uninit_array_assume_init)]

mod allocators;
mod arm_pl;
mod dtb;
mod gic;
mod linker_symbols;
mod memory;
mod utils;
mod irq_handler;

extern crate alloc;

use core::{
    arch::{asm, global_asm},
    panic::PanicInfo,
};

use crate::{
    allocators::{KBox, kbox}, arm_pl::init_from_dtb_node, dtb::{Fdt, find_earlyconsole_node}, gic::Gic, memory::init_allocator, utils::OnceSpinLock,
};

global_asm!(include_str!("asm/bootstrap.s"));
global_asm!(include_str!("asm/exception_entry.s"));

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

static GIC: OnceSpinLock<KBox<Gic>> = OnceSpinLock::new();

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

    let gic_node = root
        .children()
        .find(|node| node.name().starts_with("intc"))
        .unwrap();

    let gic = kbox(Gic::from_node(gic_node));

    gic.local_init();

    assert!(GIC.set(gic).is_ok(), "Could not initialize GIC");

    early_printk!("Done.\n");
    loop {
        unsafe { asm!("wfe") };
    }
}
