use core::{hint::spin_loop, mem::MaybeUninit, ptr::read_volatile, sync::atomic::fence};

use crate::{
    driver::{
        Driver,
        virtio::{Virtq, VirtqDescriptor},
    }, early_printk, impl_link, printk, utils::{Arc, List, ListLinks, MMIO, SpinLock, UniqueArc},
};

const DESCRIPTOR_BLOCK_SIZE: usize = 600;

const NUM_DESCRIPTORS: usize = 48;

#[repr(C)]
struct VirtqBlockRequestType {
    ty: u32,
    _reserved: u32,
    sector: u64,
}

#[repr(C)]
struct VirqBlockBuffer {
    links: ListLinks,
    buf: UniqueArc<[u8; DESCRIPTOR_BLOCK_SIZE]>,
    idx: u16,
}

impl_link!(VirqBlockBuffer, 0 => links);

struct VirtioBlkDescriptors {
    descriptors: [VirtqDescriptor; NUM_DESCRIPTORS],
    buffers: [VirqBlockBuffer; NUM_DESCRIPTORS],
    idx: usize,
}

impl VirtioBlkDescriptors {
    pub fn new() -> Self {
        let mut descriptors: [VirtqDescriptor; NUM_DESCRIPTORS] =
            [const { unsafe { core::mem::zeroed() } }; NUM_DESCRIPTORS];
        let mut buffers: [MaybeUninit<VirqBlockBuffer>; NUM_DESCRIPTORS] =
            [const { MaybeUninit::uninit() }; NUM_DESCRIPTORS];
        for (i, (descriptor, buffer)) in descriptors.iter_mut().zip(buffers.iter_mut()).enumerate()
        {
            let b = VirqBlockBuffer {
                links: ListLinks::new(),
                buf: UniqueArc::zeroed(),
                idx: i as u16,
            };
            descriptor.addr = unsafe { UniqueArc::as_ptr(&b.buf).addr() } & 0xffffffff;
            descriptor.len = DESCRIPTOR_BLOCK_SIZE as u32;
            buffer.write(b);
        }

        Self {
            descriptors,
            buffers: unsafe { MaybeUninit::from(buffers).assume_init() },
            idx: 0,
        }
    }

}

pub struct VirtioBlkDriver {
    mmio: MMIO,
    rx_queue: SpinLock<Virtq<16>>,
    rx_descriptors: SpinLock<VirtioBlkDescriptors>,
    tx_queue: SpinLock<Virtq<16>>,
    tx_descriptors: SpinLock<VirtioBlkDescriptors>,
}

impl Driver for VirtioBlkDriver {
    fn new(node: &crate::dtb::FdtNode) -> Result<Self, ()>
    where
        Self: Sized,
    {
        let addr = node.read_u64("reg", 0).unwrap();
        let size = node.read_u64("reg", 1).unwrap();

        let mmio = MMIO::new(addr as usize, size as usize);

        if unsafe { mmio.read_u32(0) } != 0x74726976 {
            return Err(());
        }
        let device_type = unsafe { mmio.read_u32(0x8) };
        if device_type != 0x2 {
            return Err(());
        }
        if unsafe { mmio.read_u32(0x4) } != 0x2 {
            return Err(());
        }

        Ok(Self {
            mmio,
            rx_descriptors: SpinLock::new(VirtioBlkDescriptors::new()),
            tx_descriptors: SpinLock::new(VirtioBlkDescriptors::new()),
            tx_queue: SpinLock::new(Virtq::zeroed()),
            rx_queue: SpinLock::new(Virtq::zeroed()),
        })
    }

    fn init(driver: Arc<Self>, _node: &crate::dtb::FdtNode)
    where
        Self: Sized,
    {
        unsafe {
            driver.mmio.write_u32(0x1, 0x70);
            driver.mmio.write_u32(0x2 | 0x1, 0x70);

            driver.mmio.write_u32(0, 0x14);
            let _ = driver.mmio.read_u32(0x10);
            driver.mmio.write_u32(0u32, 0x24);
            driver.mmio.write_u32(0u32, 0x20);
            driver.mmio.write_u32(0x8 | 0x2 | 0x1, 0x70);
            assert_eq!(driver.mmio.read_u32(0x70), 0x8 | 0x2 | 0x1);

            // Set TX queue
            driver.mmio.write_u32(0x0, 0x30);
            driver.mmio.write_u32(16, 0x38);

            driver.mmio.write_u64(
                driver.tx_descriptors.lock().descriptors.as_ptr().addr() as u64 & 0xffffffff,
                0x80,
            );
            driver
                .mmio
                .write_u64(driver.tx_queue.lock().available.addr() as u64 & 0xffffffff, 0x90);
            driver
                .mmio
                .write_u64(driver.tx_queue.lock().used.addr() as u64 & 0xffffffff, 0xa0);
            driver.mmio.write_u32(1, 0x44);

            // We're done
            driver.mmio.write_u32(0x0, 0x30);
            driver.mmio.write_u32(0xf, 0x70);
            assert_eq!(driver.mmio.read_u32(0x70), 0xf);
        }

        driver.write_blocking(12, &[0x23; 8]);
        let mut buffer = [0u8; 16];
        driver.read_blocking(12, &mut buffer);
        early_printk!("Buffer: {:?}\n", buffer);
    }

    fn compatible_string() -> &'static str
    where
        Self: Sized,
    {
        "virtio,mmio"
    }
}

impl VirtioBlkDriver {
    pub fn write_blocking(&self, sector: usize, bytes: &[u8]) {
        assert!(bytes.len() <= 512);
        unsafe { self.mmio.write_u32(0x0, 0x30); }

        let mut tx_queue = self.tx_queue.lock();
        let mut tx_descriptors = self.tx_descriptors.lock();
        let info_descriptor_idx = tx_descriptors.idx;
        let buffer_descriptor_idx = tx_descriptors.idx + 1;
        let status_descriptor_idx = tx_descriptors.idx + 2;

        tx_descriptors.descriptors[info_descriptor_idx].len = 16;
        tx_descriptors.descriptors[info_descriptor_idx].flags = 0x1;
        tx_descriptors.descriptors[info_descriptor_idx].next = buffer_descriptor_idx as u16;
        tx_descriptors.descriptors[buffer_descriptor_idx].len = 512;
        tx_descriptors.descriptors[buffer_descriptor_idx].flags = 0x1;
        tx_descriptors.descriptors[buffer_descriptor_idx].next = status_descriptor_idx as u16;
        tx_descriptors.descriptors[status_descriptor_idx].flags = 0x2;
        tx_descriptors.descriptors[status_descriptor_idx].len = 1;

        let output_descriptor = &mut tx_descriptors.buffers[info_descriptor_idx].buf;
        output_descriptor[..4].copy_from_slice(&(0x1u32.to_le_bytes()));
        output_descriptor[8..16].copy_from_slice(&(sector.to_le_bytes()));
        let output_descriptor = &mut tx_descriptors.buffers[buffer_descriptor_idx].buf;
        // let status_descriptor = &mut tx_descriptors.buffers[status_descriptor_idx].buf;

        output_descriptor[0..(0 + bytes.len())].copy_from_slice(bytes);

        tx_descriptors.idx = (tx_descriptors.idx + 3) % 16;
        let next_queue_entry = tx_queue.available.idx;

        tx_queue.available.ring[next_queue_entry as usize] = info_descriptor_idx as u16;
        fence(core::sync::atomic::Ordering::SeqCst);
        tx_queue.available.idx += 1;
        fence(core::sync::atomic::Ordering::SeqCst);

        let used_idx = tx_queue.used.idx;
        unsafe {
            self.mmio.write_u32(0, 0x50);
        }

        early_printk!("About to spin\n");

        while unsafe { read_volatile( &raw mut tx_queue.used.idx) } == used_idx {
            spin_loop();
        }
    }

    pub fn read_blocking(&self, sector: usize, bytes: &mut [u8]) {

        assert!(bytes.len() <= 512);
        unsafe { self.mmio.write_u32(0x0, 0x30); }

        let mut tx_queue = self.tx_queue.lock();
        let mut tx_descriptors = self.tx_descriptors.lock();
        let info_descriptor_idx = tx_descriptors.idx;
        let buffer_descriptor_idx = tx_descriptors.idx + 1;
        let status_descriptor_idx = tx_descriptors.idx + 2;

        tx_descriptors.descriptors[info_descriptor_idx].len = 16;
        tx_descriptors.descriptors[info_descriptor_idx].flags = 0x1;
        tx_descriptors.descriptors[info_descriptor_idx].next = buffer_descriptor_idx as u16;
        tx_descriptors.descriptors[buffer_descriptor_idx].flags = 0x2 | 0x1;
        tx_descriptors.descriptors[buffer_descriptor_idx].len = 512;
        tx_descriptors.descriptors[buffer_descriptor_idx].next = status_descriptor_idx as u16;
        tx_descriptors.descriptors[status_descriptor_idx].flags = 0x2;
        tx_descriptors.descriptors[status_descriptor_idx].len = 1;

        let info_descriptor = &mut tx_descriptors.buffers[info_descriptor_idx].buf;

        info_descriptor[..4].copy_from_slice(&(0x0u32.to_le_bytes()));
        info_descriptor[8..16].copy_from_slice(&(sector.to_le_bytes()));

        tx_descriptors.idx = (tx_descriptors.idx + 3) % 16;
        let next_queue_entry = tx_queue.available.idx;

        tx_queue.available.ring[next_queue_entry as usize] = info_descriptor_idx as u16;
        fence(core::sync::atomic::Ordering::SeqCst);
        tx_queue.available.idx += 1;
        fence(core::sync::atomic::Ordering::SeqCst);

        let used_idx = tx_queue.used.idx;
        unsafe {
            self.mmio.write_u32(0, 0x50);
        }

        early_printk!("About to spin\n");

        while unsafe { read_volatile( &raw mut tx_queue.used.idx) } == used_idx {
            spin_loop();
        }

        let buffer = &tx_descriptors.buffers[buffer_descriptor_idx].buf;
        bytes.copy_from_slice(&buffer[..bytes.len()]);
    }
}
