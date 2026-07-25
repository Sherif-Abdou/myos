mod parser;

pub use parser::{Fdt, FdtNode};

pub fn find_earlyconsole_node(fdt: &Fdt) -> &FdtNode {
    let root = fdt.root();
    let chosen = root.find_child_by_name("chosen").unwrap();

    let node_name = chosen
        .read_prop_string("stdout-path")
        .unwrap()
        .trim_start_matches('/');

    root.find_child_by_name(node_name).unwrap()
}
