use core::mem::MaybeUninit;

pub fn uninit_as_mut_slice<T>(uninit: &mut MaybeUninit<T>) -> &mut [MaybeUninit<u8>] {
    unsafe {
        core::slice::from_raw_parts_mut(
            uninit.as_mut_ptr() as *mut MaybeUninit<u8>,
            core::mem::size_of::<T>(),
        )
    }
}

pub fn copy_to_uninit<T>(uninit: &mut MaybeUninit<T>, src: &[u8]) {
    let dst = uninit_as_mut_slice(uninit);

    let src =
        unsafe { core::slice::from_raw_parts(src.as_ptr() as *const MaybeUninit<u8>, src.len()) };

    dst.copy_from_slice(src);
}
