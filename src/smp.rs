use core::arch::asm;

use crate::{
    allocators::KBox,
    cpu_local, printk,
    sched::{KernelTaskStack, create_kernel_stack},
    utils::{CpuLocal, OnceSpinLock},
};

static STACKS: CpuLocal<OnceSpinLock<KBox<KernelTaskStack>>> = cpu_local!(OnceSpinLock::new());

unsafe extern "C" {
    unsafe static _secondary_start: usize;
}

pub fn bringup_core(number: usize) {
    let stack = create_kernel_stack();
    let stack_end = stack.virt_addr() + stack.len();

    let entry_addr = (&raw const _secondary_start).addr();
    if STACKS.get(number).set(stack).is_err() {
        return;
    };

    unsafe {
        asm!(
            r#"
            ldr x0, =0xC4000003
            hvc #0
        "#,
            in("x1") number,
            in("x2") entry_addr,
            in("x3") stack_end,
            out("x0") _,
            out("x4") _,
            out("x5") _,
            out("x6") _,
            out("x7") _,
        )
    }
}
