#![no_std]
#![no_main]
#![allow(dead_code)]
#![feature(allocator_api)]
#![feature(maybe_uninit_array_assume_init)]

mod allocators;
mod arm_pl;
mod dtb;
mod interrupts;
mod linker_symbols;
mod memory;
mod timer;
mod utils;
mod sched;

extern crate alloc;

use core::{
    arch::{asm, global_asm},
    panic::PanicInfo,
};

use crate::{
    allocators::{KBox, kbox}, arm_pl::init_from_dtb_node, dtb::{Fdt, find_earlyconsole_node}, interrupts::{Gic, configure_exceptions, daifclr}, memory::init_allocator, sched::init_scheduler, timer::ArmTimer, utils::OnceSpinLock,
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

fn timer() {}

#[unsafe(no_mangle)]
extern "C" fn entry() {
    configure_exceptions();
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

    ArmTimer::enable();
    ArmTimer::wait(100_000);

    GIC.get().unwrap().enable_local_ppi(27);
    Gic::set_local_priority(0xff);
    Gic::enable_local_interrupts();

    init_scheduler();

    daifclr();
    early_printk!("Done.\n");
    loop {
        unsafe { asm!("wfe") };
    }
}
