use core::{
    fmt::Write,
    sync::atomic::{AtomicU32, Ordering::SeqCst},
};

use crate::{
    driver::Driver,
    impl_link,
    sched::Mutex,
    subsystem::{ConsoleDeviceFile, devtmpfs},
    utils::{Arc, KString, LazySpinLock, List, ListArc, ListLinks, UniqueArc},
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
    static CONSOLE_COUNT: AtomicU32 = AtomicU32::new(0);
    let console = Console {
        driver: console,
        links: ListLinks::new(),
    };
    let console: ListArc<Console, 0> = UniqueArc::new(console).into();
    let console_inode = console.clone_arc();
    CONSOLES.get().push_console(console);

    let index = CONSOLE_COUNT.fetch_add(1, SeqCst);
    let mut name = KString::new();
    let _ = write!(name, "console{}", index);

    devtmpfs().push_operation(crate::subsystem::DevTmpFsOp::AddDevice {
        name,
        ops: Arc::new(ConsoleDeviceFile::new(console_inode)),
    });
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
