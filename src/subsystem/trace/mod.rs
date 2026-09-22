use core::{
    fmt::Write,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    per_cpu_lock,
    subsystem::{FsError, InodeOperations},
    utils::{MAX_CPUS, PerCpuLock},
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

    pub fn extend(&mut self, buf: &[u8]) {
        let size = self.size;
        let remaining = TRACE_BUFFER_SIZE - size;
        let to_write = buf.len().min(remaining);
        self.buffer[size..(size + to_write)].copy_from_slice(&buf[..to_write]);
        self.size += to_write;
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
        let to_read = len.min(available);

        buf[..to_read].copy_from_slice(&self.buffer[self.cursor..self.cursor + to_read]);

        self.cursor += to_read;

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
        self.cursor += s.len();
        Ok(())
    }
}

macro_rules! parse_tracecmd {
    ($cmd:expr, $cmd_target:expr => $str:expr, $read:expr, $write:expr, $name:tt : $param_type:tt $(,$rest_name:tt : $rest_type:ty)* $(,)?) => {
        if $cmd == ($cmd_target as u16) {
            let mut timestamp_buf = [0u8; 8];
            let succeeded = $read.read(&mut timestamp_buf);
            if !succeeded {
                return Ok($write.cursor);
            }
            let timestamp = u64::from_le_bytes(timestamp_buf);

            let _ = write!($write, "[{}.{:06}] {}\t| ", timestamp / 1_000_000, timestamp % 1_000_000, $str);
            parse_tracecmd!($read, $write, $name : $param_type, $($rest_name : $rest_type),*);
        }
    };
    ($read:expr, $write:expr, $name:tt : u32, $($rest_name:tt : $rest_type:ty),*) => {
        let mut buf = [0u8; 4];
        let succeeded = $read.read(&mut buf);
        if !succeeded {
            return Ok($write.cursor);
        }
        let number = u32::from_le_bytes(buf);
        let _ = write!($write, "{}: {}\n", stringify!($name), number);

        parse_tracecmd!($($rest_name : $rest_type),*);
    };
    ($read:expr, $write:expr, $name:tt : u64, $($rest_name:tt : $rest_type:ty),*) => {
        let mut buf = [0u8; 8];
        let succeeded = $read.read(&mut buf);
        if !succeeded {
            return Ok($write.cursor);
        }
        let number = u64::from_le_bytes(buf);
        let _ = write!($write, "{}: {}\n", stringify!($name), number);

        parse_tracecmd!($($rest_name : $rest_type),*);
    };
    () => {}
}

impl InodeOperations for TraceBuffer {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> super::FsResult<usize> {
        if offset > 0 {
            return Err(FsError::EndOfFile);
        }

        let mut read_cursor = Cursor::new(self.buffer.as_slice(), 0);
        let mut write_cursor = CursorMut::new(buffer, 0);

        loop {
            let mut cmd_buf = [0u8; 2];
            let succeeded = read_cursor.read(&mut cmd_buf);
            if !succeeded {
                break;
            }
            let cmd = u16::from_le_bytes(cmd_buf);
            if cmd == 0 {
                break;
            }
            parse_tracecmd!(cmd, Trace::SchedEntry => "SchedEntry", read_cursor, write_cursor, pid : u32);
            parse_tracecmd!(cmd, Trace::SchedExit => "SchedExit", read_cursor, write_cursor, pid : u32);
        }

        Ok(write_cursor.cursor)
    }
}

pub static TRACE_ENABLED: AtomicBool = AtomicBool::new(false);

pub struct TraceEnableFile {}

impl InodeOperations for TraceEnableFile {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> super::FsResult<usize> {
        if offset > 0 {
            return Err(FsError::EndOfFile);
        }

        let enabled = TRACE_ENABLED.load(Ordering::Relaxed);
        let mut cursor = CursorMut::new(buffer, 0);

        if enabled {
            let _ = cursor.write_str("y\n");
        } else {
            let _ = cursor.write_str("n\n");
        }
        Ok(cursor.cursor)
    }

    fn write(&self, offset: u64, buffer: &[u8]) -> super::FsResult<usize> {
        if offset > 0 {
            return Err(FsError::EndOfFile);
        }

        if buffer.is_empty() {
            return Ok(0);
        }

        if buffer[0] == b'1' || buffer[0] == b'y' {
            TRACE_ENABLED.store(true, Ordering::Relaxed);
            Ok(1)
        } else if buffer[0] == b'0' || buffer[0] == b'n' {
            TRACE_ENABLED.store(false, Ordering::Relaxed);
            Ok(1)
        } else {
            Ok(0)
        }
    }
}

pub struct TraceClearFile {}

impl InodeOperations for TraceClearFile {
    fn write(&self, offset: u64, buffer: &[u8]) -> super::FsResult<usize> {
        if offset > 0 {
            return Err(FsError::EndOfFile);
        }

        let _ = buffer;

        for cpu in 0..MAX_CPUS {
            // Race conditions are whatever in this case.
            let mut trace_buffer = unsafe { TRACE_BUFFERS.lock_cpu(cpu) };

            trace_buffer.buffer.fill(0);
        }

        Ok(0)
    }
}

pub struct TraceBufferFile {
    cpu: usize,
}

impl TraceBufferFile {
    pub const fn new(cpu: usize) -> Self {
        Self { cpu }
    }
}

impl InodeOperations for TraceBufferFile {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> super::FsResult<usize> {
        // Just accept we may read inconsistent data.
        unsafe { TRACE_BUFFERS.lock_cpu(self.cpu).read(offset, buffer) }
    }
}

pub static TRACE_BUFFERS: PerCpuLock<TraceBuffer> = per_cpu_lock!(TraceBuffer::new());

#[macro_export]
macro_rules! trace {
    ($cmd:expr,$($rest:expr),* $(,)?) => {
        if $crate::subsystem::trace::TRACE_ENABLED.load(core::sync::atomic::Ordering::Relaxed) {
            let mut trace_buffer = $crate::subsystem::trace::TRACE_BUFFERS.lock();
            trace_buffer.extend(&($cmd as u16).to_le_bytes());
            trace_buffer.extend(&($crate::timer::ArmTimer::now()).to_le_bytes());
            trace!(@buffer = trace_buffer, $($rest),*);
        }
    };
    (@buffer = $trace_buffer:expr, $param:expr $(,$rest:expr)* $(,)?) => {
        $trace_buffer.extend(&$param.to_le_bytes());
    };
}

#[repr(u16)]
pub enum Trace {
    Eob = 0,
    SchedEntry = 1,
    SchedExit,
}
