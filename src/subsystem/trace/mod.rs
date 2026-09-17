use core::fmt::Write;

use crate::{
    allocators::KBox,
    per_cpu_lock,
    subsystem::InodeOperations,
    utils::{Deque, PerCpuLock},
};

const TRACE_BUFFER_SIZE: usize = 16 * 1024;

pub struct TraceBuffer {
    buffer: [u8; TRACE_BUFFER_SIZE],
    size: usize,
}

impl TraceBuffer {
    const fn new() -> Self {
        Self {
            buffer: [0u8; _],
            size: 0,
        }
    }

    pub fn write(&mut self, buf: &[u8]) {
        let size = self.size;
        let end = (self.size + buf.len()).min(self.buffer.len());
        self.buffer[size..end].copy_from_slice(&buf[..(end.saturating_sub(buf.len()))]);
    }
}

struct CursorMut<'a> {
    buffer: &'a mut [u8],
    cursor: usize,
}

impl<'a> CursorMut<'a> {
    fn new(buffer: &'a mut [u8], cursor: usize) -> Self {
        Self { buffer, cursor }
    }
}

struct Cursor<'a> {
    buffer: &'a [u8],
    cursor: usize,
}

impl<'a> Cursor<'a> {
    fn new(buffer: &'a [u8], cursor: usize) -> Self {
        Self { buffer, cursor }
    }

    fn read(&mut self, buf: &mut [u8]) -> bool {
        let len = buf.len();
        let available = self.buffer.len() - self.cursor;
        let to_read = len.max(available);

        buf[..to_read].copy_from_slice(&self.buffer[self.cursor..self.cursor + to_read]);

        buf.len() == to_read
    }
}

impl Write for CursorMut<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let len = s.len();
        let available = self.buffer.len() - self.cursor;
        if len > available {
            return Err(core::fmt::Error);
        }

        self.buffer[self.cursor..self.cursor + s.len()].copy_from_slice(s.as_bytes());
        Ok(())
    }
}

macro_rules! parse_tracecmd {
    ($cmd:expr, $cmd_target:expr, $read:expr, $write:expr, $name:tt : $param_type:ty,$($rest_name:tt : $rest_type:ty),*) => {
        if $cmd == $cmd_target { parse_tracecmd!($read, $write, $name : $param_type, $($rest_name : $rest_type),*); }
    };
    ($read:expr, $write:expr, $name:tt : $param_type:ty,$($rest:tt),*) => {
        let mut buf = [0u8; 4];
        let succeeded = $read.read(&mut buf);
        if !succeeded {
            return Ok($read.cursor);
        }
        let number = u32::from_le_bytes(buf);
        let _ = write!($write, "{}", stringify!($name));
        let _ = write!($write, "{}", number);

        parse_tracecmd!($($rest),*);
    };
    () => {}
}

impl InodeOperations for TraceBuffer {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> super::FsResult<usize> {
        let _ = offset;

        let mut read_cursor = Cursor::new(self.buffer.as_slice(), 0);
        let mut write_cursor = CursorMut::new(buffer, 0);

        loop {
            let mut cmd_buf = [0u8; 2];
            let succeeded = read_cursor.read(&mut cmd_buf);
            if !succeeded {
                break;
            }
            let cmd = u16::from_le_bytes(cmd_buf);
            parse_tracecmd!(cmd, 1, read_cursor, write_cursor, pid : u32,);
        }

        Ok(0)
    }
}

pub static TRACE_BUFFERS: PerCpuLock<TraceBuffer> = per_cpu_lock!(TraceBuffer::new());
