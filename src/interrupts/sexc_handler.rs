use core::{arch::asm, slice::SliceIndex};

use crate::{
    early_printk,
    interrupts::{
        ExceptionRegisters, RETURN_TABLE,
        syscalls::{dispatch_syscall, validate_user_address_range},
    },
    per_cpu_lock, printk, read_sysreg,
    sched::SCHEDULER,
    subsystem::{PageFaultError, PageFaultType},
    utils::{PerCpuLock, with_core_critical_section},
    write_sysreg,
};

type SexcHandler = fn(*mut ExceptionRegisters) -> *const ExceptionRegisters;
struct SexcTable([SexcHandler; 64]);

fn with_uaccess<R, F: FnOnce() -> R>(func: F) -> R {
    unsafe {
        write_sysreg!(PAN, 0 << 22);
    }

    let ret = func();

    unsafe {
        write_sysreg!(PAN, 1 << 22);
    }

    ret
}

impl PerCpuLock<SexcTable> {
    pub fn dispatch(&self, ec: usize, regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
        self.lock().0[ec](regs)
    }

    pub fn set(&self, ec: usize, func: SexcHandler) -> SexcHandler {
        let mut old: SexcHandler = func;

        core::mem::swap(&mut old, &mut self.lock().0[ec]);

        old
    }
}

const fn create_sexc_table() -> SexcTable {
    let mut base_table = SexcTable([const { default_sexc_handler }; 64]);

    base_table.0[0x15] = dispatch_syscall;
    base_table.0[0x20] = user_instruction_fault_handler;
    base_table.0[0x24] = user_data_fault_handler;

    base_table
}

pub struct UserInput<T>(T);

impl UserInput<&[u8]> {
    pub unsafe fn from_cstr(user_address: usize, max_len: usize) -> Result<Self, ()> {
        if !validate_user_address_range(user_address, 0) {
            return Err(());
        }

        let len = user_strnlen(user_address as _, max_len);

        if !validate_user_address_range(user_address, len * size_of::<u8>()) {
            return Err(());
        }

        Ok(Self(unsafe {
            core::slice::from_raw_parts(user_address as _, len)
        }))
    }
}

impl<T> UserInput<&[T]> {
    pub unsafe fn from_raw_parts(user_address: usize, user_len: usize) -> Result<Self, ()> {
        if !validate_user_address_range(user_address, user_len.saturating_mul(size_of::<T>())) {
            return Err(());
        }

        Ok(Self(unsafe {
            core::slice::from_raw_parts(user_address as _, user_len)
        }))
    }

    /// Returns how many elements of type T were fully copied.
    pub fn copy_to_slice(&self, dst: &mut [T]) -> usize {
        let (_, src, _) = unsafe { self.0.align_to::<u8>() };
        let (_, dst, _) = unsafe { dst.align_to_mut::<u8>() };

        copy_from_user(dst, src) / core::mem::size_of::<T>()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<T> UserInput<&[T]> {
    pub fn index<I>(&self, index: I) -> UserInput<&'_ [T]>
    where
        I: SliceIndex<[T], Output = [T]>,
    {
        UserInput(&self.0[index])
    }
}

impl<T> UserInput<&mut [T]> {
    pub fn index<I>(&self, index: I) -> UserInput<&[T]>
    where
        I: SliceIndex<[T], Output = [T]>,
    {
        UserInput(&self.0[index])
    }
}

impl<T> UserInput<&mut [T]> {
    pub fn index_mut<I>(&'_ mut self, index: I) -> UserInput<&'_ mut [T]>
    where
        I: SliceIndex<[T], Output = [T]>,
    {
        UserInput(&mut self.0[index])
    }
}

impl<T> UserInput<&mut [T]> {
    pub unsafe fn from_raw_parts_mut(user_address: usize, user_len: usize) -> Result<Self, ()> {
        if !validate_user_address_range(user_address, user_len.saturating_mul(size_of::<T>())) {
            return Err(());
        }

        Ok(Self(unsafe {
            core::slice::from_raw_parts_mut(user_address as _, user_len)
        }))
    }

    /// Returns how many elements of type T were fully copied.
    pub fn copy_to_slice(&self, dst: &mut [T]) -> usize {
        let (_, src, _) = unsafe { self.0.align_to::<u8>() };
        let (_, dst, _) = unsafe { dst.align_to_mut::<u8>() };

        copy_from_user(dst, src) / core::mem::size_of::<T>()
    }

    /// Returns how many elements of type T were fully copied.
    pub fn copy_from_slice(&mut self, src: &[T]) -> usize {
        let (_, src, _) = unsafe { src.align_to::<u8>() };
        let (_, dst, _) = unsafe { self.0.align_to_mut::<u8>() };

        copy_to_user(dst, src) / core::mem::size_of::<T>()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub fn copy_from_user(dst: &mut [u8], src: &[u8]) -> usize {
    with_core_critical_section(|| {
        with_uaccess(|| {
            let old_func = SEXC_TABLE.set(0x25, user_data_access_handler);

            let size = copy_from_user_inner(dst, src);

            let _ = SEXC_TABLE.set(0x25, old_func);

            size
        })
    })
}

fn copy_from_user_inner(dst: &mut [u8], src: &[u8]) -> usize {
    let mut len = 0;
    let mut failed: u64 = 0;

    while len < src.len() {
        let mut byte: u8;
        let ptr = src[len..].as_ptr();
        unsafe {
            asm!(
                r#"
                mov x0, #0
                ldrb {0:w}, [{1}]
                "#,
                out(reg) byte,
                in(reg) ptr.addr(),
                out("x0") failed,
            );
        }
        if failed != 0 {
            return len;
        }
        dst[len] = byte;

        len += 1;
    }

    len
}

pub fn copy_to_user(dst: &mut [u8], src: &[u8]) -> usize {
    with_core_critical_section(|| {
        with_uaccess(|| {
            let old_func = SEXC_TABLE.set(0x25, user_data_access_handler);

            let size = copy_to_user_inner(dst, src);

            let _ = SEXC_TABLE.set(0x25, old_func);

            size
        })
    })
}

fn copy_to_user_inner(dst: &mut [u8], src: &[u8]) -> usize {
    let mut len = 0;
    let mut failed: u64 = 0;

    while len < src.len() {
        let byte: u8 = src[len];
        let ptr = dst[len..].as_ptr();
        unsafe {
            asm!(
                r#"
                mov x0, #0
                strb {0:w}, [{1}]
                "#,
                in(reg) byte,
                in(reg) ptr.addr(),
                out("x0") failed,
            );
        }
        if failed != 0 {
            printk!("Failed to copy from user\n");
            return len;
        }

        len += 1;
    }

    len
}

fn user_strnlen_inner(src: *const u8, max_len: usize) -> usize {
    let mut len = 0;
    let mut failed: u64 = 0;

    while len < max_len {
        let byte: u8;
        let ptr = unsafe { src.byte_add(len) };
        unsafe {
            asm!(
                r#"
                mov x0, #0
                ldrb {0:w}, [{1}]
                "#,
                out(reg) byte,
                in(reg) ptr.addr(),
                out("x0") failed,
            );
        }
        if failed != 0 {
            return len;
        }

        if byte != 0 {
            len += 1;
        } else {
            return len;
        }
    }

    len
}

pub fn user_strnlen(src: *const u8, max_len: usize) -> usize {
    with_core_critical_section(|| {
        with_uaccess(|| {
            let old_func = SEXC_TABLE.set(0x25, user_data_access_handler);

            let size = user_strnlen_inner(src, max_len);

            let _ = SEXC_TABLE.set(0x25, old_func);

            size
        })
    })
}

fn handle_data_access_fault(
    regs: *mut ExceptionRegisters,
    ignore_el: bool,
) -> Result<*const ExceptionRegisters, PageFaultError> {
    let esr: u64;
    unsafe {
        read_sysreg!(esr, ESR_EL1);
    }

    let far: u64;
    unsafe {
        read_sysreg!(far, FAR_EL1);
    }

    let iss = esr & 0x1ffffff;

    let fsc = iss & 0x3f;

    // Translation fault.
    let interrupted_user = unsafe { (*regs).spsr & 0b1111 } == 0;
    if (4..8).contains(&fsc)
        && (interrupted_user || !ignore_el)
        && let Some(task) = SCHEDULER.get().unwrap().local_task()
        && task.is_user_task()
    {
        if task
            .handle_page_fault(PageFaultType::Translation, far as usize)
            .is_ok()
        {
            Ok(regs)
        } else {
            Err(PageFaultError::Unhandled)
        }
    } else if (12..16).contains(&fsc)
        && (interrupted_user || !ignore_el)
        && let Some(task) = SCHEDULER.get().unwrap().local_task()
        && task.is_user_task()
    {
        if task
            .handle_page_fault(PageFaultType::Permission, far as usize)
            .is_ok()
        {
            Ok(regs)
        } else {
            Err(PageFaultError::Unhandled)
        }
    } else {
        Err(PageFaultError::Unhandled)
    }
}

fn user_data_fault_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    if let Ok(regs) = handle_data_access_fault(regs, true) {
        regs
    } else {
        default_sexc_handler(regs)
    }
}

fn user_instruction_fault_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    if let Ok(regs) = handle_data_access_fault(regs, true) {
        regs
    } else {
        default_sexc_handler(regs)
    }
}

fn user_data_access_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    if let Ok(handled) = handle_data_access_fault(regs, false) {
        return handled;
    }

    let esr: u64;
    unsafe {
        read_sysreg!(esr, ESR_EL1);
    }

    let iss = esr & 0x1ffffff;
    let dfsc = iss & 0x3f;

    // Translation fault.
    if (4..8).contains(&dfsc) || (12..16).contains(&dfsc) {
        // We don't swap to disk yet, so cancel the copy
        unsafe { (*regs).gprs[0] = 0x1 }
        unsafe { (*regs).elr += 0x4 }

        regs
    } else {
        default_sexc_handler(regs)
    }
}

static SEXC_TABLE: PerCpuLock<SexcTable> = per_cpu_lock!(create_sexc_table());

#[unsafe(no_mangle)]
extern "C" fn sexc_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    early_printk!("EXCEPTION: \n");

    let esr: u64;
    unsafe {
        read_sysreg!(esr, ESR_EL1);
    }

    let ec = (esr >> 26) & 0x3f;

    SEXC_TABLE.dispatch(ec as usize, regs)
}

fn default_sexc_handler(regs: *mut ExceptionRegisters) -> *const ExceptionRegisters {
    let far: u64;
    unsafe {
        read_sysreg!(far, FAR_EL1);
    }
    let elr: u64;
    unsafe {
        read_sysreg!(elr, ELR_EL1);
    }
    let esr: u64;
    unsafe {
        read_sysreg!(esr, ESR_EL1);
    }

    let ec = (esr >> 26) & 0x3f;
    let interrupted_user = unsafe { (*regs).spsr & 0b1111 } == 0;
    if interrupted_user
        && let Some(task) = SCHEDULER.get().unwrap().local_task()
        && task.is_user_task()
    {
        printk!("SIGSEV\n");
        abort_thread()
    } else {
        dispatch_kernel_panic(regs, far, elr, esr, ec)
    }
}

fn abort_thread() -> *const ExceptionRegisters {
    SCHEDULER.get().unwrap().end_task(-1);

    RETURN_TABLE
        .lock()
        .put(&SCHEDULER.get().unwrap().next_task().unwrap());

    RETURN_TABLE.lock().take().unwrap()
}

fn dispatch_kernel_panic(
    regs: *mut ExceptionRegisters,
    far: u64,
    elr: u64,
    esr: u64,
    ec: u64,
) -> ! {
    printk!(
        "EXCEPTION at 0x{:x}, FAR 0x{:x}, ESR 0x{:x} \n",
        elr,
        far,
        esr
    );
    unsafe {
        printk!("x0: {:x}\n", (*regs).gprs[0]);
        printk!("x1: {:x}\n", (*regs).gprs[1]);
        printk!("x2: {:x}\n", (*regs).gprs[2]);
        printk!("x3: {:x}\n", (*regs).gprs[3]);
        printk!("x4: {:x}\n", (*regs).gprs[4]);
        printk!("x5: {:x}\n", (*regs).gprs[5]);
        printk!("x6: {:x}\n", (*regs).gprs[6]);
        printk!("x7: {:x}\n", (*regs).gprs[7]);
        printk!("x8: {:x}\n", (*regs).gprs[8]);
        printk!("x9: {:x}\n", (*regs).gprs[9]);
        printk!("x10: {:x}\n", (*regs).gprs[10]);
        printk!("x11: {:x}\n", (*regs).gprs[11]);
        printk!("x12: {:x}\n", (*regs).gprs[12]);
        printk!("x13: {:x}\n", (*regs).gprs[13]);
        printk!("x14: {:x}\n", (*regs).gprs[14]);
        printk!("x15: {:x}\n", (*regs).gprs[15]);
        printk!("x16: {:x}\n", (*regs).gprs[16]);
        printk!("x17: {:x}\n", (*regs).gprs[17]);
        printk!("x18: {:x}\n", (*regs).gprs[18]);
        printk!("x19: {:x}\n", (*regs).gprs[19]);
        printk!("x20: {:x}\n", (*regs).gprs[20]);
        printk!("x21: {:x}\n", (*regs).gprs[21]);
        printk!("x22: {:x}\n", (*regs).gprs[22]);
        printk!("x23: {:x}\n", (*regs).gprs[23]);
        printk!("x24: {:x}\n", (*regs).gprs[24]);
        printk!("x25: {:x}\n", (*regs).gprs[25]);
        printk!("x26: {:x}\n", (*regs).gprs[28]);
        printk!("x27: {:x}\n", (*regs).gprs[27]);
        printk!("x28: {:x}\n", (*regs).gprs[28]);
        printk!("x29: {:x}\n", (*regs).gprs[29]);
        printk!("x30: {:x}\n", (*regs).gprs[30]);
        printk!("sp: {:x}\n", (*regs).gprs[31]);
    }

    match ec {
        0x1 => {
            printk!("Trapped WF* Instruction\n");
        }
        0x3 => {
            printk!("Trapped MCR or MRC\n");
        }
        0x7 => {
            printk!("Trapped FPU\n");
        }
        0xd => {
            printk!("Branch target exception\n");
        }
        0xe => {
            printk!("Illegal execution state\n");
        }
        0x15 => {
            printk!("Trapped SVC\n");
        }
        0x20 => {
            printk!("EL0 Instruction Abort\n");
        }
        0x21 => {
            printk!("EL1 Instruction Abort\n");
        }
        0x24 => {
            printk!("EL0 Data Abort\n");
        }
        0x25 => {
            printk!("EL1 Data Abort\n");
        }
        _ => {
            printk!("EC: {:x}\n", ec);
        }
    }

    loop {
        unsafe {
            asm!("wfi");
        }
    }
}
