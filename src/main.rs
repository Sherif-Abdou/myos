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
mod elf;
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
    allocators::{KBox, kbox},
    arm_pl::init_from_dtb_node,
    driver::DeviceBus,
    dtb::{Fdt, find_earlyconsole_node},
    interrupts::{Gic, IRQ_TABLE, RETURN_TABLE, configure_exceptions, daifclr},
    memory::init_allocator,
    sched::{SCHEDULER, init_scheduler},
    subsystem::{EXT2_FS, Ext2Fs, build_kernel_page_table},
    timer::{ArmTimer, TIMER_QUEUE, TimerQueue, us_sleep},
    utils::{ArcAny, OnceSpinLock},
};

global_asm!(include_str!("asm/bootstrap.s"));
global_asm!(include_str!("asm/exception_entry.s"));

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    early_printk!("PANIC\n");
    early_printk!("{}\n", info.message());
    printk!("{}\n", info.message());

    loop {
        unsafe { asm!("wfe") };
    }
}

static GIC: OnceSpinLock<KBox<Gic>> = OnceSpinLock::new();

static FDT: OnceSpinLock<Fdt> = OnceSpinLock::new();

fn periodic_timer_handler(_arc: Option<&ArcAny>) {
    let timer_queue = TIMER_QUEUE.get().unwrap();
    timer_queue.enqueue(10_000, periodic_timer_handler, None);

    if let Some(new_ret) = SCHEDULER.get().unwrap().next_task() {
        SCHEDULER.get().unwrap().flush_kill_queue();
        *RETURN_TABLE.lock() = Some(new_ret);
    }
}

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

    GIC.get().unwrap().enable_local_ppi(27);
    Gic::set_local_priority(0xff);
    Gic::enable_local_interrupts();

    let _ = TIMER_QUEUE.set(TimerQueue::new());

    TIMER_QUEUE
        .get()
        .unwrap()
        .enqueue(10_000, periodic_timer_handler, None);

    IRQ_TABLE.lock().register_interrupt(
        27,
        |_| {
            let timer_queue = TIMER_QUEUE.get().unwrap();
            let event = timer_queue.pop();

            if let Some(event) = event {
                event.dispatch();
            }

            timer_queue.wait_for_next();
        },
        None,
    );

    assert!(FDT.set(fdt).is_ok());

    init_scheduler();

    early_printk!("Scheduler initialized, starting full boot.\n");
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

pub fn threaded_init(_arg: *mut ()) {
    daifclr();
    let fdt = FDT.get().unwrap();

    build_kernel_page_table(fdt.root());

    assert!(DEVICE_BUS.set(DeviceBus::new()).is_ok());

    DEVICE_BUS.get().unwrap().walk_fdt_root(fdt.root());

    printk!("Walked DTS\n");

    let _ = EXT2_FS.set(Ext2Fs::new());

    static ELF_FILE: &[u8] = include_bytes!("../usr/main");

    // SCHEDULER.get().unwrap().load_program(ELF_FILE);

    printk!("Kernel initialized\n");

    printk!("a\n");
    us_sleep(1_000_000);
    printk!("b\n");
    us_sleep(1_000_000);
    printk!("c\n");
    loop {
        unsafe {
            asm!("wfi");
        }
    }
}
