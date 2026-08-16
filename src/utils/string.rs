use core::{ffi::CStr, str::Chars};

use crate::allocators::{KERNEL_ALLOCATOR, KVec};

pub fn str_from_cstr(bytes: &[u8]) -> &str {
    CStr::from_bytes_until_nul(bytes)
        .expect("No null terminator")
        .to_str()
        .expect("Could not parse utf-8")
}

/// Ascii-only heap allocated owned string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KString {
    buf: KVec<u8>,
}

impl KString {
    pub fn new() -> Self {
        KString {
            buf: KVec::new_in(&KERNEL_ALLOCATOR),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        KString {
            buf: KVec::with_capacity_in(capacity, &KERNEL_ALLOCATOR),
        }
    }

    pub fn from_str(string: &str) -> Self {
        let mut allocated_string = Self::with_capacity(string.len());

        unsafe {
            allocated_string.buf.set_len(string.len());
        }

        allocated_string.buf.copy_from_slice(string.as_bytes());

        allocated_string
    }

    pub fn as_str(&self) -> &str {
        unsafe { str::from_utf8_unchecked(&self.buf) }
    }

    pub fn push_byte(&mut self, value: u8) {
        self.buf.push(value);
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn chars(&self) -> Chars<'_> {
        self.as_str().chars()
    }
}

impl AsRef<str> for KString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
