use core::{any::Any, mem::MaybeUninit, ptr::read_volatile, sync::atomic::fence};

use crate::{
    GIC, driver::{
        Driver,
        virtio::{Virtq, VirtqDescriptor},
    }, dtb::FdtNode, early_printk, impl_link, interrupts::{IRQ_TABLE, daifclr}, printk, sched::{Mutex, WaitQueue}, subsystem::{BlockDriver, set_disk}, utils::{Arc, ListLinks, Mmio, SpinLock, UniqueArc},
};

const DESCRIPTOR_BLOCK_SIZE: usize = 600;

const NUM_DESCRIPTORS: usize = 48;

const DESCRIPTOR_COUNT: usize = 16;

fn irq_handler(driver: Option<&Arc<dyn Any + Send + Sync + 'static>>) {
    let driver: &VirtioBlkDriver = driver
        .map(|driver| driver.downcast_ref::<VirtioBlkDriver>().unwrap())
        .expect("Driver must be passed in");

    let status = unsafe { driver.mmio.read_u32(0x60) };


    if status & (1 << 0) != 0 {
        driver.wait_queue.unblock_front();
    }

    unsafe {
        driver.mmio.write_u32(status, 0x64);
    }
}

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
    mmio: Mmio,
    queue: SpinLock<Virtq<DESCRIPTOR_COUNT>>,
    descriptors: SpinLock<VirtioBlkDescriptors>,
    wait_queue: WaitQueue,
    // This driver only supports one thread having a request in flight at a time.
    device_lock: Mutex<()>,
}

impl Driver for VirtioBlkDriver {
    fn new(node: &crate::dtb::FdtNode) -> Result<Self, ()>
    where
        Self: Sized,
    {
        let addr = node.read_u64("reg", 0).unwrap();
        let size = node.read_u64("reg", 1).unwrap();

        let mmio = Mmio::new(addr as usize, size as usize);

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
            descriptors: SpinLock::new(VirtioBlkDescriptors::new()),
            wait_queue: WaitQueue::new(),
            queue: SpinLock::new(Virtq::zeroed()),
            device_lock: Mutex::new(()),
        })
    }

    fn init(driver: Arc<Self>, node: &crate::dtb::FdtNode)
    where
        Self: Sized,
    {
        driver.init_virtio();
        Self::init_irq(&driver, node);

        set_disk(driver.clone());
    }

    fn compatible_string() -> &'static str
    where
        Self: Sized,
    {
        "virtio,mmio"
    }
}

impl VirtioBlkDriver {
    fn init_virtio(&self) {
        unsafe {
            self.mmio.write_u32(0x1, 0x70);
            self.mmio.write_u32(0x2 | 0x1, 0x70);

            self.mmio.write_u32(0, 0x14);
            let _ = self.mmio.read_u32(0x10);
            self.mmio.write_u32(0u32, 0x24);
            self.mmio.write_u32(0u32, 0x20);
            self.mmio.write_u32(0x8 | 0x2 | 0x1, 0x70);
            assert_eq!(self.mmio.read_u32(0x70), 0x8 | 0x2 | 0x1);

            // Set TX queue
            self.mmio.write_u32(0x0, 0x30);
            self.mmio.write_u32(DESCRIPTOR_COUNT as u32, 0x38);

            self.mmio.write_u64(
                self.descriptors.lock().descriptors.as_ptr().addr() as u64 & 0xffffffff,
                0x80,
            );
            self.mmio
                .write_u64(self.queue.lock().available.addr() as u64 & 0xffffffff, 0x90);
            self.mmio
                .write_u64(self.queue.lock().used.addr() as u64 & 0xffffffff, 0xa0);
            self.mmio.write_u32(1, 0x44);

            // We're done
            self.mmio.write_u32(0x0, 0x30);
            self.mmio.write_u32(0xf, 0x70);
            assert_eq!(self.mmio.read_u32(0x70), 0xf);
        }
    }

    fn init_irq(driver: &Arc<Self>, fdt: &FdtNode) {
        let irqn = fdt.read_u32("interrupts", 1).unwrap();


        IRQ_TABLE
            .lock()
            .register_interrupt((irqn + 32) as u64, irq_handler, Some(driver.clone()));
        GIC.get().unwrap().configure_spi(irqn as usize + 32, 0b10);
        GIC.get().unwrap().enable_spi(irqn as usize + 32);
    }
}

impl BlockDriver for VirtioBlkDriver {
    fn write_sector(&self, sector: u64, bytes: &[u8]) {
        assert!(bytes.len() <= 512);
        let _lock = self.device_lock.lock();

        unsafe {
            self.mmio.write_u32(0x0, 0x30);
        }

        let mut queue = self.queue.lock();
        let mut descriptors = self.descriptors.lock();
        let info_descriptor_idx = descriptors.idx;
        let buffer_descriptor_idx = (descriptors.idx + 1) % DESCRIPTOR_COUNT;
        let status_descriptor_idx = (descriptors.idx + 2) % DESCRIPTOR_COUNT;

        descriptors.descriptors[info_descriptor_idx].len = 16;
        descriptors.descriptors[info_descriptor_idx].flags = 0x1;
        descriptors.descriptors[info_descriptor_idx].next = buffer_descriptor_idx as u16;
        descriptors.descriptors[buffer_descriptor_idx].len = 512;
        descriptors.descriptors[buffer_descriptor_idx].flags = 0x1;
        descriptors.descriptors[buffer_descriptor_idx].next = status_descriptor_idx as u16;
        descriptors.descriptors[status_descriptor_idx].flags = 0x2;
        descriptors.descriptors[status_descriptor_idx].len = 1;

        let output_descriptor = &mut descriptors.buffers[info_descriptor_idx].buf;
        output_descriptor[..4].copy_from_slice(&(0x1u32.to_le_bytes()));
        output_descriptor[8..16].copy_from_slice(&(sector.to_le_bytes()));
        let output_descriptor = &mut descriptors.buffers[buffer_descriptor_idx].buf;

        output_descriptor[..bytes.len()].copy_from_slice(bytes);

        descriptors.idx = (descriptors.idx + 3) % DESCRIPTOR_COUNT;
        let next_queue_entry = queue.available.idx;

        queue.available.ring[next_queue_entry as usize] = info_descriptor_idx as u16;
        fence(core::sync::atomic::Ordering::SeqCst);
        queue.available.idx += 1;
        fence(core::sync::atomic::Ordering::SeqCst);

        let used_idx = queue.used.idx;
        unsafe {
            self.mmio.write_u32(0, 0x50);
        }
        drop(descriptors);
        drop(queue);

        while unsafe { read_volatile(&raw mut self.queue.lock().used.idx) } == used_idx {
            self.wait_queue.enqueue_and_block();
        }
    }

    fn read_sector(&self, sector: u64, bytes: &mut [u8]) {
        assert!(bytes.len() <= 512);
        let _lock = self.device_lock.lock();

        unsafe {
            self.mmio.write_u32(0x0, 0x30);
        }

        let mut queue = self.queue.lock();
        let mut descriptors = self.descriptors.lock();
        let info_descriptor_idx = descriptors.idx;
        let buffer_descriptor_idx = (descriptors.idx + 1) % DESCRIPTOR_COUNT;
        let status_descriptor_idx = (descriptors.idx + 2) % DESCRIPTOR_COUNT;

        descriptors.descriptors[info_descriptor_idx].len = 16;
        descriptors.descriptors[info_descriptor_idx].flags = 0x1;
        descriptors.descriptors[info_descriptor_idx].next = buffer_descriptor_idx as u16;
        descriptors.descriptors[buffer_descriptor_idx].flags = 0x2 | 0x1;
        descriptors.descriptors[buffer_descriptor_idx].len = 512;
        descriptors.descriptors[buffer_descriptor_idx].next = status_descriptor_idx as u16;
        descriptors.descriptors[status_descriptor_idx].flags = 0x2;
        descriptors.descriptors[status_descriptor_idx].len = 1;

        let info_descriptor = &mut descriptors.buffers[info_descriptor_idx].buf;

        info_descriptor[..4].copy_from_slice(&(0x0u32.to_le_bytes()));
        info_descriptor[8..16].copy_from_slice(&(sector.to_le_bytes()));

        descriptors.idx = (descriptors.idx + 3) % DESCRIPTOR_COUNT;
        let next_queue_entry = queue.available.idx;

        queue.available.ring[next_queue_entry as usize] = info_descriptor_idx as u16;
        fence(core::sync::atomic::Ordering::SeqCst);
        queue.available.idx += 1;
        fence(core::sync::atomic::Ordering::SeqCst);

        let used_idx = queue.used.idx;
        unsafe {
            self.mmio.write_u32(0, 0x50);
        }
        drop(descriptors);
        drop(queue);

        while unsafe { read_volatile(&raw mut self.queue.lock().used.idx) } == used_idx {
            self.wait_queue.enqueue_and_block();
        }

        let buffer = &self.descriptors.lock().buffers[buffer_descriptor_idx].buf;
        bytes.copy_from_slice(&buffer[..bytes.len()]);
    }

}
