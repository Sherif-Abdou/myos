use crate::utils::{MAX_CPUS, cpu_id};

#[macro_export]
macro_rules! cpu_local {
    ($value:expr) => {
        $crate::utils::CpuLocal::new([const { $value }; _])
    };
}

pub struct CpuLocal<T> {
    inner: [T; MAX_CPUS],
}

impl<T> CpuLocal<T> {
    pub const fn new(inner: [T; MAX_CPUS]) -> Self {
        Self { inner }
    }

    /// I think this is only true if called in a context
    /// where you can't block or be rescheduled.
    ///
    /// Audit usage of this for now
    pub fn local(&self) -> &T {
        let cpu = cpu_id();

        &self.inner[cpu]
    }

    pub fn get(&self, cpu: usize) -> &T {
        &self.inner[cpu]
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter()
    }
}
