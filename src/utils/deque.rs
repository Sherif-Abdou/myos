use core::mem::MaybeUninit;

pub struct Deque<T, const N: usize> {
    buffer: [MaybeUninit<T>; N],
    start: usize,
    end: usize,
    len: usize,
}

impl<T, const N: usize> Deque<T, N> {
    pub const fn new() -> Self {
        Self {
            buffer: [const { MaybeUninit::zeroed() }; N],
            start: 0,
            end: 0,
            len: 0,
        }
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub const fn is_full(&self) -> bool {
        self.len() == N
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn push(&mut self, value: T) {
        assert!(!self.is_full());
        let index = self.end;

        self.buffer[index].write(value);
        self.end = (self.end + 1) % N;
    }

    pub const fn front(&self) -> &T {
        unsafe { self.buffer[self.start].assume_init_ref() }
    }

    pub fn pop(&mut self) {
        assert!(!self.is_empty());

        let index = self.start;
        if core::mem::needs_drop::<T>() {
            unsafe {
                self.buffer[index].assume_init_drop();
            }
        }
        self.start = (self.start + 1) % N;
        self.len -= 1;
    }

    pub fn pop_many(&mut self, count: usize) {
        assert!(count <= self.len());

        if core::mem::needs_drop::<T>() {
            for i in 0..count {
                let index = i % N;
                unsafe {
                    self.buffer[index].assume_init_drop();
                }
            }
        }

        self.start = (self.start + count) % N;
        self.len -= count;
    }

    pub fn as_slices(&self) -> (&[T], &[T]) {
        self.slices_with_count(self.len())
    }

    pub fn slices_with_count(&self, count: usize) -> (&[T], &[T]) {
        assert!(count <= self.len());
        let end = (self.start + count) % (N + 1);
        unsafe {
            if self.start < self.end {
                (self.buffer[self.start..end].assume_init_ref(), &[])
            } else {
                (
                    self.buffer[self.start..].assume_init_ref(),
                    self.buffer[..end].assume_init_ref(),
                )
            }
        }
    }
}
