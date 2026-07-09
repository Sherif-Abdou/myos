use crate::{early_printk, gic::Gic};

#[unsafe(no_mangle)]
extern "C" fn sexc_handler() {}

#[unsafe(no_mangle)]
extern "C" fn irq_handler() {
    let irq = Gic::acknowledge();
    early_printk!("Hi\n");

    Gic::complete(irq);
}
