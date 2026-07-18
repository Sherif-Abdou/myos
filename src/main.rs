#![no_std]
#![no_main]
#![allow(dead_code)]
#![feature(allocator_api)]
#![feature(maybe_uninit_array_assume_init)]
#![feature(unsize)]
#![feature(coerce_unsized)]

mod allocators;
mod arm_pl;
mod driver;
mod dtb;
mod interrupts;
mod linker_symbols;
mod memory;
mod sched;
mod timer;
mod utils;

extern crate alloc;

use core::{
    arch::{asm, global_asm},
    panic::PanicInfo,
};

use crate::{
    allocators::{KBox, kbox},
    arm_pl::init_from_dtb_node,
    driver::DeviceBus,
    dtb::{Fdt, find_earlyconsole_node},
    interrupts::{Gic, IRQ_TABLE, RETURN_TABLE, configure_exceptions, daifclr},
    memory::init_allocator,
    sched::{SCHEDULER, init_scheduler},
    timer::ArmTimer,
    utils::OnceSpinLock,
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

static FDT: OnceSpinLock<Fdt> = OnceSpinLock::new();

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

    IRQ_TABLE.lock().register_interrupt(
        27,
        |_| {
            ArmTimer::wait(1_000_000);
            early_printk!("Timer\n");

            if let Some(new_ret) = SCHEDULER.get().unwrap().next_task() {
                *RETURN_TABLE.lock() = Some(new_ret);
            }
        },
        None,
    );

    assert!(FDT.set(fdt).is_ok());

    init_scheduler();

    early_printk!("Scheduler initialized, starting full boot.\n");
    SCHEDULER.get().unwrap().task_from_fn(threaded_init);
    daifclr();

    loop {
        unsafe { asm!("wfi") };
    }
}

pub static DEVICE_BUS: OnceSpinLock<DeviceBus> = OnceSpinLock::new();

pub fn threaded_init(_arg: *mut ()) {
    let fdt = FDT.get().unwrap();

    assert!(DEVICE_BUS.set(DeviceBus::new()).is_ok());

    DEVICE_BUS.get().unwrap().walk_fdt_root(fdt.root());

    loop {
        unsafe {
            asm!("wfi");
        }
    }
}
