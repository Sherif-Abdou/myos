use core::fmt::Write;

use crate::{
    driver::Driver,
    impl_link,
    sched::Mutex,
    utils::{Arc, LazySpinLock, List, ListArc, ListLinks, UniqueArc},
};

pub trait ConsoleDriver: Driver {
    fn write_bytes(&self, buf: &[u8]);
    fn write_str(&self, s: &str) -> core::fmt::Result {
        self.write_bytes(s.as_bytes());

        Ok(())
    }

    fn read(&self, buf: &mut [u8]) -> usize {
        let _ = buf;

        0
    }
}

pub struct Console {
    driver: Arc<dyn ConsoleDriver + Send + Sync + 'static>,
    links: ListLinks,
}

impl_link!(Console, 0 => links);

impl Console {
    pub fn read(&self, buf: &mut [u8]) -> usize {
        self.driver.read(buf)
    }
}

impl Write for &Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.driver.write_str(s)
    }
}

impl Write for Arc<Console> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        (&**self).write_str(s)
    }
}

pub fn set_kernel_console() {
    CONSOLES.get().pick_first_kernel_console();
}

pub fn add_console(console: Arc<dyn ConsoleDriver + Send + Sync + 'static>) {
    let console = Console {
        driver: console,
        links: ListLinks::new(),
    };
    CONSOLES.get().push_console(UniqueArc::new(console));
}

pub struct ConsoleSubsystem {
    consoles: Mutex<List<Console>>,
    kernel_console: Mutex<Option<Arc<Console>>>,
}

impl ConsoleSubsystem {
    fn new() -> Self {
        Self {
            consoles: Mutex::new(List::new()),
            kernel_console: Mutex::new(None),
        }
    }

    pub fn push_console(&self, console: impl Into<ListArc<Console, 0>>) {
        let console = console.into();
        self.consoles.lock().push_back(console);
    }

    pub fn set_kernel_console(&self, console: Arc<Console>) {
        *self.kernel_console.lock() = Some(console);
    }

    pub fn kernel_console(&self) -> Option<Arc<Console>> {
        let console = self.kernel_console.lock();
        let inner = console.as_ref();

        inner.cloned()
    }

    fn pick_first_kernel_console(&self) {
        let first_console = self
            .consoles
            .lock()
            .cursor()
            .get_arc()
            .expect("No console available\n");

        CONSOLES.get().set_kernel_console(first_console);
    }
}

pub static CONSOLES: LazySpinLock<ConsoleSubsystem> = LazySpinLock::new(ConsoleSubsystem::new);

#[macro_export]
macro_rules! printk {
    ($($x:expr),*) => {
        {
        use ::core::fmt::Write;
        let console_locked = $crate::subsystem::CONSOLES.get().kernel_console();
        if let Some(console) = console_locked.as_ref() {
            let console_ref = &console;
            let _ = ::core::write!(&***console_ref, $($x,)*);
        }
        }
    };
}
