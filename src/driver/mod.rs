mod pl;
mod virtio;

use crate::{
    driver::{pl::Pl, virtio::VirtioBlkDriver},
    dtb::FdtNode,
    impl_link,
    utils::{Arc, List, ListArc, ListLinks, SpinLock, UniqueArc},
};

pub struct Device {
    driver: Arc<dyn Driver>,
    tag: &'static str,
    link: ListLinks,
}

impl_link!(Device, 0 => link);

pub trait Driver {
    fn new(node: &FdtNode) -> Result<Self, ()>
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
            let Ok(driver) = $driver::new($node) else {
                return;
            };
            let driver: UniqueArc<$driver> = UniqueArc::new(driver);
            let driver: ListArc<$driver, 0> = driver.into();
            $driver::init(driver.clone_arc(), $node);

            let device = UniqueArc::new(Device {
                driver: driver.clone_arc(),
                tag: $compatible,
                link: ListLinks::new(),
            });

            $devices.lock().push_back(device.into());
            $node.set_probed(true);
            return;
        }
    };
}

impl DeviceBus {
    pub fn new() -> Self {
        Self {
            devices: SpinLock::new(List::new()),
        }
    }

    fn early_fdt_walk(&self, root: &FdtNode) {
        let chosen = root.find_child_by_name("chosen").unwrap();

        let node_name = chosen
            .read_prop_string("stdout-path")
            .unwrap()
            .trim_start_matches('/');

        if let Some(node) = root.find_child_by_name(node_name) {
            self.try_probe_node(node);
        }
    }

    fn try_probe_node(&self, node: &FdtNode) {
        if node.is_probed() {
            return;
        }

        let Some(compatible) = node.read_prop_string("compatible") else {
            return;
        };

        check_driver!(self.devices, node, compatible, Pl);
        check_driver!(self.devices, node, compatible, VirtioBlkDriver);
    }

    pub fn walk_fdt_root(&self, root: &FdtNode) {
        self.early_fdt_walk(root);

        for node in root.children() {
            self.try_probe_node(node);
        }
    }
}
