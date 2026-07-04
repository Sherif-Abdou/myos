mod parser;

pub use parser::{Fdt, FdtNode, FdtProp};

use crate::early_printk;

pub fn find_earlyconsole_node(fdt: &Fdt) -> &FdtNode {
    let root = fdt.nodes().next().unwrap();
    let chosen = root
        .children()
        .find(|node| node.name() == "chosen")
        .unwrap();

    let stdout_path = chosen
        .properties()
        .find(|prop| prop.name() == "stdout-path")
        .unwrap();

    let node_name = stdout_path
        .read_prop_string()
        .unwrap()
        .trim_start_matches('/');

    root.children()
        .find(|node| node.name() == node_name)
        .unwrap()
}
