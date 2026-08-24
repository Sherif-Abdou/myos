use crate::{
    allocators::{KVec, kvec},
    cpu_local, read_sysreg,
    sched::WaitQueue,
    utils::{Arc, ArcAny, CpuLocal, OnceSpinLock, SpinLock, expect_downcast_ref},
    write_sysreg,
};

pub struct ArmTimer {}

impl ArmTimer {
    pub fn enable() {
        let mut enable: u64 = 0;
        unsafe {
            read_sysreg!(enable, CNTV_CTL_EL0);
            enable |= 1;
            write_sysreg!(CNTV_CTL_EL0, enable);
        }
    }

    pub fn disable() {
        let mut enable: u64 = 0;
        unsafe {
            read_sysreg!(enable, CNTV_CTL_EL0);
            enable &= !1;
            write_sysreg!(CNTV_CTL_EL0, enable);
        }
    }

    pub fn wait_for(micros: u64) {
        let mut freq = 0u64;
        unsafe {
            read_sysreg!(freq, CNTFRQ_EL0);

            let ticks = (micros * freq) / 1_000_000;
            write_sysreg!(CNTV_TVAL_EL0, ticks);
        }
    }

    pub fn wait_until(micros: u64) {
        let mut freq = 0u64;
        unsafe {
            read_sysreg!(freq, CNTFRQ_EL0);

            let ticks = (((micros as u128) * (freq as u128)) / 1_000_000) as u64;
            write_sysreg!(CNTV_CVAL_EL0, ticks);
        }
    }

    pub fn now() -> u64 {
        let mut freq = 0u64;
        let mut time = 0u64;

        unsafe {
            read_sysreg!(time, CNTVCT_EL0);
            read_sysreg!(freq, CNTFRQ_EL0);
        }

        ((time as u128 * 1_000_000) / freq as u128) as u64
    }
}

pub static TIMER_QUEUE: CpuLocal<OnceSpinLock<TimerQueue>> = cpu_local!(OnceSpinLock::new());

pub struct TimerQueue {
    heap: SpinLock<TimerMinHeap<TimerItem>>,
}

impl TimerQueue {
    pub fn new() -> Self {
        Self {
            heap: SpinLock::new(TimerMinHeap::new()),
        }
    }

    pub fn enqueue(&self, delay_us: u64, func: fn(Option<&ArcAny>), data: Option<ArcAny>) {
        let target_time = ArmTimer::now() + delay_us;
        let mut queue = self.heap.lock();

        queue.push(TimerItem {
            target_time,
            func,
            data,
        });

        drop(queue);

        self.wait_for_next();
    }

    pub fn wait_for_next(&self) {
        let queue = self.heap.lock();

        if let Some(front) = queue.peek() {
            ArmTimer::enable();

            ArmTimer::wait_until(front.target_time);
        } else {
            ArmTimer::disable();
        }
    }

    pub fn pop(&self) -> Option<TimerItem> {
        let mut queue = self.heap.lock();
        if !queue.is_empty() {
            let item = queue.pop();

            Some(item)
        } else {
            None
        }
    }
}

pub struct TimerItem {
    target_time: u64,
    func: fn(Option<&ArcAny>),
    data: Option<ArcAny>,
}

impl TimerItem {
    pub fn dispatch(&self) {
        (self.func)(self.data.as_ref())
    }
}

impl PartialEq for TimerItem {
    fn eq(&self, other: &Self) -> bool {
        self.target_time == other.target_time
    }
}

impl PartialOrd for TimerItem {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.target_time.partial_cmp(&other.target_time)
    }
}

pub struct TimerMinHeap<T: PartialOrd> {
    inner: KVec<T>,
}

impl<T: PartialOrd> TimerMinHeap<T> {
    pub fn new() -> Self {
        Self { inner: kvec() }
    }

    pub fn push(&mut self, value: T) {
        let mut index = self.inner.len();
        self.inner.push(value);

        while index > 0 {
            let parent = (index - 1) / 2;

            if self.inner[parent] > self.inner[index] {
                let (upper, lower) = self.inner.split_at_mut(index);

                core::mem::swap(&mut upper[parent], &mut lower[0]);
            }

            index = parent;
        }
    }

    pub fn peek(&self) -> Option<&T> {
        self.inner.first()
    }

    pub fn pop(&mut self) -> T {
        let ret = self.inner.swap_remove(0);
        let len = self.len();

        let mut index = 0;
        while index < len {
            let left_child = 2 * index + 1;
            let right_child = 2 * index + 2;

            let mut next_index = index;

            if left_child < len && self.inner[index] > self.inner[left_child] {
                let (upper, lower) = self.inner.split_at_mut(left_child);

                core::mem::swap(&mut upper[index], &mut lower[0]);
                next_index = left_child;
            }

            if right_child < len && self.inner[index] > self.inner[right_child] {
                let (upper, lower) = self.inner.split_at_mut(right_child);

                core::mem::swap(&mut upper[index], &mut lower[0]);
                next_index = right_child;
            }

            if next_index == index {
                break;
            } else {
                index = next_index;
            }
        }

        ret
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub fn us_sleep(delay_us: u64) {
    let wait_queue = Arc::new(WaitQueue::new());

    TIMER_QUEUE.local().get().unwrap().enqueue(
        delay_us,
        |arg| {
            let wait_queue: &WaitQueue = expect_downcast_ref(arg);

            wait_queue.unblock_all();
        },
        Some(wait_queue.clone()),
    );

    wait_queue.enqueue_and_block();
}

pub fn ms_sleep(delay_ms: u64) {
    us_sleep(delay_ms * 1_000);
}
