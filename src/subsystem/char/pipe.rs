use core::sync::atomic::{AtomicU32, Ordering::SeqCst};

use crate::{
    sched::{Mutex, WaitQueue},
    subsystem::{FsError, InodeOperations},
    utils::{Arc, Deque},
};

const PIPE_SIZE: usize = 1024;

pub struct Pipe {
    buf: Mutex<Deque<u8, PIPE_SIZE>>,
    tx_waiters: WaitQueue,
    rx_waiters: WaitQueue,
    senders: AtomicU32,
    receivers: AtomicU32,
}

impl Pipe {
    pub fn new() -> Self {
        Self {
            buf: Mutex::new(Deque::new()),
            tx_waiters: WaitQueue::new(),
            rx_waiters: WaitQueue::new(),
            senders: AtomicU32::new(0),
            receivers: AtomicU32::new(0),
        }
    }

    pub fn pipe() -> (ReadPipe, WritePipe) {
        let inner = Arc::new(Self {
            buf: Mutex::new(Deque::new()),
            tx_waiters: WaitQueue::new(),
            rx_waiters: WaitQueue::new(),
            senders: AtomicU32::new(1),
            receivers: AtomicU32::new(1),
        });

        (ReadPipe(inner.clone()), WritePipe(inner))
    }
}

pub struct ReadPipe(Arc<Pipe>);

impl Clone for ReadPipe {
    fn clone(&self) -> Self {
        self.0.receivers.fetch_add(1, SeqCst);
        Self(self.0.clone())
    }
}

impl Drop for ReadPipe {
    fn drop(&mut self) {
        if self.0.receivers.fetch_sub(1, SeqCst) == 1 {
            self.0.rx_waiters.unblock_all();
        }
    }
}

impl InodeOperations for ReadPipe {
    fn read(&self, offset: &mut u64, buffer: &mut [u8]) -> crate::subsystem::FsResult<usize> {
        self.0.read(offset, buffer)
    }
}

pub struct WritePipe(Arc<Pipe>);

impl Clone for WritePipe {
    fn clone(&self) -> Self {
        self.0.senders.fetch_add(1, SeqCst);
        Self(self.0.clone())
    }
}

impl Drop for WritePipe {
    fn drop(&mut self) {
        if self.0.senders.fetch_sub(1, SeqCst) == 1 {
            self.0.tx_waiters.unblock_all();
        }
    }
}

impl InodeOperations for WritePipe {
    fn write(&self, offset: &mut u64, buffer: &[u8]) -> crate::subsystem::FsResult<usize> {
        self.0.write(offset, buffer)
    }
}

impl InodeOperations for Pipe {
    fn read(&self, offset: &mut u64, buffer: &mut [u8]) -> crate::subsystem::FsResult<usize> {
        let _ = offset;

        loop {
            let mut pipe = self.buf.lock();

            if pipe.is_empty() {
                if self.senders.load(core::sync::atomic::Ordering::SeqCst) == 0 {
                    return Err(FsError::EndOfFile);
                }

                self.tx_waiters.enqueue();
                drop(pipe);
                self.tx_waiters.block();
                continue;
            }

            let (slice_a, slice_b) = pipe.as_slices();
            let to_read_in_a = slice_a.len().min(buffer.len());
            let to_read_in_b = slice_b.len().min(buffer.len() - to_read_in_a);
            buffer[..to_read_in_a].copy_from_slice(&slice_a[..to_read_in_a]);
            if to_read_in_a < buffer.len() {
                buffer[to_read_in_a..to_read_in_a + to_read_in_b]
                    .copy_from_slice(&slice_b[..to_read_in_b]);
            }

            pipe.pop_many(to_read_in_a + to_read_in_b);

            self.rx_waiters.unblock_all();

            return Ok(to_read_in_a + to_read_in_b);
        }
    }

    fn write(&self, offset: &mut u64, buffer: &[u8]) -> crate::subsystem::FsResult<usize> {
        let _ = offset;

        loop {
            let mut pipe = self.buf.lock();

            if pipe.free() == 0 {
                if self.receivers.load(core::sync::atomic::Ordering::SeqCst) == 0 {
                    return Err(FsError::EndOfFile);
                }
                self.rx_waiters.enqueue();
                drop(pipe);
                self.rx_waiters.block();
                continue;
            }

            let mut bytes = 0;
            while !pipe.is_full() && bytes < buffer.len() {
                pipe.push(buffer[bytes]);
                bytes += 1;
            }

            self.tx_waiters.unblock_all();

            return Ok(bytes);
        }
    }
}
