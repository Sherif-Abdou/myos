mod pipe;

pub use pipe::*;

use core::fmt::Write;

use crate::{
    subsystem::{CONSOLES, Console, FsResult, InodeOperations},
    utils::Arc,
};

pub struct KernelConsoleDeviceFile;

impl InodeOperations for KernelConsoleDeviceFile {
    fn read(&self, offset: &mut u64, buffer: &mut [u8]) -> FsResult<usize> {
        let _ = offset;

        Ok(CONSOLES.get().kernel_console().unwrap().read(buffer))
    }

    fn write(&self, offset: &mut u64, buffer: &[u8]) -> FsResult<usize> {
        let _ = offset;

        let _ = (&*CONSOLES.get().kernel_console().unwrap())
            .write_str(unsafe { str::from_utf8_unchecked(buffer) });

        Ok(buffer.len())
    }
}

pub struct ConsoleDeviceFile(Arc<Console>);

impl ConsoleDeviceFile {
    pub fn new(console: Arc<Console>) -> Self {
        Self(console)
    }
}

impl InodeOperations for ConsoleDeviceFile {
    fn read(&self, offset: &mut u64, buffer: &mut [u8]) -> FsResult<usize> {
        let _ = offset;

        Ok(self.0.read(buffer))
    }

    fn write(&self, offset: &mut u64, buffer: &[u8]) -> FsResult<usize> {
        let _ = offset;

        let _ = (&*self.0).write_str(unsafe { str::from_utf8_unchecked(buffer) });

        Ok(buffer.len())
    }
}
