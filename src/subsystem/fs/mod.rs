mod ext2;
mod inode;
mod tmpfs;

use crate::{
    sched::MutexGuard,
    utils::{Arc, List},
};

pub use ext2::read_super_block;
pub use inode::*;
pub use tmpfs::TmpFs;

pub trait FileSystem {
    fn root(&self) -> Arc<Inode>;

    fn create_file(&self, name: &str) {
        self.root().create_file(name);
    }

    fn remove_file(&self, name: &str) {
        self.root().remove_file(name);
    }

    fn create_directory(&self, name: &str) {
        self.root().create_directory(name);
    }

    fn remove_directory(&self, name: &str) {
        self.root().remove_directory(name);
    }

    fn list_directory<R, F: FnOnce(MutexGuard<'_, List<Inode>>) -> R>(&self, func: F) -> R {
        self.root().list_directory(func)
    }

    fn open(&self, path: &str) -> Option<Arc<Inode>> {
        let parts = path.split("/");

        let mut current = self.root();
        for part in parts {
            if part.is_empty() {
                continue;
            }

            let potential_child = current.list_directory(|mut list| {
                let mut cursor = list.cursor_mut();
                while cursor
                    .get()
                    .is_some_and(|child| child.meta().name() != part)
                {
                    let _ = cursor.next();
                }

                cursor.get_arc()
            });

            let child = potential_child?;
            current = child
        }
        Some(current)
    }

    fn create(&self, path: &str) -> Option<Arc<Inode>> {
        let parts = path.split("/");
        let num_parts = path.chars().filter(|c| *c == '/').count();

        let mut current = self.root();
        for part in parts.take(num_parts - 1) {
            if part.is_empty() {
                continue;
            }

            let potential_child = current.list_directory(|mut list| {
                let mut cursor = list.cursor_mut();
                while cursor
                    .get()
                    .is_some_and(|child| child.meta().name() != part)
                {
                    let _ = cursor.next();
                }

                cursor.get_arc()
            });

            let child = potential_child?;
            current = child
        }

        let child_to_create = path.split('/').nth(num_parts - 1)?;
        current.create_file(child_to_create);

        current.list_directory(|mut list| {
            let mut cursor = list.cursor_mut();
            while cursor
                .get()
                .is_some_and(|child| child.meta().name() != child_to_create)
            {
                let _ = cursor.next();
            }

            cursor.get_arc()
        })
    }
}
