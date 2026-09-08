mod pipe;

pub use pipe::*;

use core::fmt::Write;

use crate::subsystem::{CONSOLE, FsResult, InodeOperations};

pub struct ConsoleDeviceFile;

impl InodeOperations for ConsoleDeviceFile {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let _ = offset;

        Ok(CONSOLE.get().unwrap().read(buffer))
    }

    fn write(&self, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let _ = offset;

        let _ = CONSOLE
            .get()
            .unwrap()
            .write_str(unsafe { str::from_utf8_unchecked(buffer) });

        Ok(buffer.len())
    }
}
