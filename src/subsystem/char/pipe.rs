use crate::{
    sched::{Mutex, WaitQueue},
    subsystem::InodeOperations,
    utils::Deque,
};

const PIPE_SIZE: usize = 128;

pub struct Pipe {
    buf: Mutex<Deque<u8, PIPE_SIZE>>,
    tx_waiters: WaitQueue,
    rx_waiters: WaitQueue,
}

impl Pipe {
    pub fn new() -> Self {
        Self {
            buf: Mutex::new(Deque::new()),
            tx_waiters: WaitQueue::new(),
            rx_waiters: WaitQueue::new(),
        }
    }
}

impl InodeOperations for Pipe {
    fn read(&self, offset: u64, buffer: &mut [u8]) -> crate::subsystem::FsResult<usize> {
        let _ = offset;

        loop {
            let mut pipe = self.buf.lock();

            if pipe.is_empty() {
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

    fn write(&self, offset: u64, buffer: &[u8]) -> crate::subsystem::FsResult<usize> {
        let _ = offset;

        loop {
            let mut pipe = self.buf.lock();

            if pipe.free() == 0 {
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
