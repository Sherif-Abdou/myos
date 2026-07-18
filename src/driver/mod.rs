mod pl;
mod virtio;

use crate::{
    driver::pl::Pl,
    dtb::FdtNode,
    early_printk, impl_link,
    utils::{Arc, List, ListArc, ListLinks, SpinLock, UniqueArc},
};

pub struct Device {
    driver: Arc<dyn Driver>,
    tag: &'static str,
    link: ListLinks,
}

impl_link!(Device, 0 => link);

pub trait Driver {
    fn new(node: &FdtNode) -> Self
    where
        Self: Sized;

    fn init(driver: Arc<Self>, node: &FdtNode)
    where
        Self: Sized;

    fn compatible_string() -> &'static str
    where
        Self: Sized;
}

pub struct DeviceBus {
    devices: SpinLock<List<Device>>,
}

macro_rules! check_driver {
    ($devices:expr,$node:expr,$compatible:expr,$driver:tt) => {
        if $compatible == $driver::compatible_string() {
            early_printk!("Probing {}\n", $node.name());

            let driver: UniqueArc<$driver> = UniqueArc::new($driver::new($node));
            let driver: ListArc<$driver, 0> = driver.into();
            $driver::init(driver.clone_arc(), $node);

            let device = UniqueArc::new(Device {
                driver: driver.clone_arc(),
                tag: $compatible,
                link: ListLinks::new(),
            });

            $devices.lock().push_back(device.into());
        }
    };
}

impl DeviceBus {
    pub fn new() -> Self {
        Self {
            devices: SpinLock::new(List::new()),
        }
    }

    pub fn walk_fdt_root(&self, root: &FdtNode) {
        for node in root.children() {
            let Some(compatible) = node.read_prop_string("compatible") else {
                continue;
            };

            check_driver!(self.devices, node, compatible, Pl);
        }
    }
}
