#![no_std]
#![no_main]
#![allow(dead_code)]
#![feature(allocator_api)]
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
mod subsystem;
mod timer;
mod utils;

extern crate alloc;

use core::{
    arch::{asm, global_asm},
    panic::PanicInfo,
};

use crate::{
    allocators::{KBox, kbox}, arm_pl::init_from_dtb_node, driver::DeviceBus, dtb::{Fdt, find_earlyconsole_node}, interrupts::{Gic, IRQ_TABLE, RETURN_TABLE, configure_exceptions, daifclr}, memory::init_allocator, sched::{SCHEDULER, init_scheduler}, subsystem::{FileSystem, TmpFs, block_cache}, timer::ArmTimer, utils::OnceSpinLock,
};

global_asm!(include_str!("asm/bootstrap.s"));
global_asm!(include_str!("asm/exception_entry.s"));

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_printk!("PANIC\n");
    if let Some(message) = info.message().as_str() {
        early_printk!("{}\n", message);
        printk!("{}\n", message);
    }

    loop {
        unsafe { asm!("wfe") };
    }
}

static GIC: OnceSpinLock<KBox<Gic>> = OnceSpinLock::new();

static FDT: OnceSpinLock<Fdt> = OnceSpinLock::new();

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
            ArmTimer::wait(1_000);

            if let Some(new_ret) = SCHEDULER.get().unwrap().next_task() {
                *RETURN_TABLE.lock() = Some(new_ret);
            }
        },
        None,
    );

    assert!(FDT.set(fdt).is_ok());

    init_scheduler();

    early_printk!("Scheduler initialized, starting full boot.\n");
    SCHEDULER
        .get()
        .unwrap()
        .task_from_fn(threaded_idle, core::ptr::null_mut());
    SCHEDULER
        .get()
        .unwrap()
        .task_from_fn(threaded_init, core::ptr::null_mut());
    daifclr();

    loop {
        unsafe { asm!("wfi") };
    }
}

pub static DEVICE_BUS: OnceSpinLock<DeviceBus> = OnceSpinLock::new();

pub fn threaded_idle(_arg: *mut ()) {
    daifclr();

    loop {
        unsafe {
            asm!("wfi");
        }
    }
}

pub fn threaded_init(_arg: *mut ()) {
    daifclr();
    let fdt = FDT.get().unwrap();

    assert!(DEVICE_BUS.set(DeviceBus::new()).is_ok());

    DEVICE_BUS.get().unwrap().walk_fdt_root(fdt.root());

    printk!("Walked DTS\n");

    let mut data = [0u8; 16];
    block_cache().read(12 * 512, &mut data);
    block_cache().write(12 * 512, &[1u8; 16]);
    printk!("Read data: {:?}\n", data);

    let fs = TmpFs::new();
    fs.create_file("hello.txt");
    fs.create_file("hello2.txt");

    printk!("Done\n");
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}
