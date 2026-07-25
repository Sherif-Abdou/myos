mod block;

pub use block::{BlockCache, BlockDriver, set_disk, block_cache};

use core::fmt::Write;

use crate::{
    driver::Driver,
    utils::{Arc, OnceSpinLock},
};

pub trait ConsoleDriver: Driver {
    fn write_bytes(&self, buf: &[u8]);
    fn write_str(&self, s: &str) -> core::fmt::Result {
        self.write_bytes(s.as_bytes());

        Ok(())
    }
}

pub struct Console(Arc<dyn ConsoleDriver + Send + Sync + 'static>);

impl Write for &Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.write_str(s)
    }
}

pub fn set_console(console: Arc<dyn ConsoleDriver + Send + Sync + 'static>) {
    let _ = CONSOLE.set(Console(console));
}

pub static CONSOLE: OnceSpinLock<Console> = OnceSpinLock::new();

#[macro_export]
macro_rules! printk {
    ($($x:expr),*) => {
        {
        use ::core::fmt::Write;
        let console_locked = $crate::subsystem::CONSOLE.get();
        if let Some(console) = console_locked.as_ref() {
            let mut console_ref = *console;
            let _ = ::core::write!(console_ref, $($x,)*);
        }
        }
    };
}
