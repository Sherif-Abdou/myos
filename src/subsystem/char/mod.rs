mod pipe;

pub use pipe::*;

use core::fmt::Write;

use crate::subsystem::{CONSOLES, FsResult, InodeOperations};

pub struct ConsoleDeviceFile;

impl InodeOperations for ConsoleDeviceFile {
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
