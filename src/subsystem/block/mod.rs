use core::sync::atomic::{AtomicUsize, Ordering::SeqCst};

use crate::{
    impl_link,
    sched::{Mutex, MutexGuard, WaitQueue, Workqueue},
    utils::{Arc, ArcAny, List, ListLinks, OnceSpinLock, UniqueArc, expect_downcast_ref},
};

pub trait BlockDriver {
    fn read_sector(&self, sector: u64, buf: &mut [u8]);

    fn write_sector(&self, sector: u64, buf: &[u8]);
}

static DISK: OnceSpinLock<Arc<dyn BlockDriver + Send + Sync + 'static>> = OnceSpinLock::new();

static BLOCK_CACHE: OnceSpinLock<BlockCache> = OnceSpinLock::new();

const MAX_CACHE_SIZE: usize = 1024 * 64;

pub fn set_disk(disk: Arc<dyn BlockDriver + Send + Sync + 'static>) {
    let _ = DISK.set(disk);
    let _ = BLOCK_CACHE.set(BlockCache::new());
}

pub fn block_cache() -> &'static BlockCache {
    BLOCK_CACHE.get().unwrap()
}

const BLOCK_SIZE: usize = 512;

struct BlockSectorEntryInner {
    sector: u64,
    buffer: [u8; BLOCK_SIZE],
    fetch_in_progress: bool,
    flush_in_progress: bool,
    dirty: bool,
}

impl BlockSectorEntryInner {
    pub const fn new(sector: u64) -> Self {
        Self {
            sector,
            buffer: [0; BLOCK_SIZE],
            fetch_in_progress: false,
            flush_in_progress: false,
            dirty: false,
        }
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Updates the contents of this block from disk.
    ///
    /// This marks the block as clean.
    pub fn reset_from_disk<R>(&mut self, f: impl FnOnce(&mut [u8]) -> R) -> R {
        let ret = f(self.buffer.as_mut_slice());
        self.dirty = false;

        ret
    }

    /// Modifies the contents of this block from a user.
    ///
    /// This marks the page as dirty.
    pub fn modify_block<R>(&mut self, f: impl FnOnce(&mut [u8]) -> R) -> R {
        let ret = f(self.buffer.as_mut_slice());
        self.dirty = true;

        ret
    }

    pub fn mark_flushed(&mut self) {
        self.dirty = false;
        self.flush_in_progress = false;
    }

    pub fn mark_flush_in_progress(&mut self) {
        self.flush_in_progress = true;
    }

    pub fn is_flush_in_progress(&self) -> bool {
        self.flush_in_progress
    }
}

struct BlockSectorEntry {
    inner: Mutex<BlockSectorEntryInner>,
    wait_queue: WaitQueue,
    link: ListLinks,
}

impl_link!(BlockSectorEntry, 0 => link);

impl BlockSectorEntry {
    const fn new(sector: u64) -> Self {
        Self {
            inner: Mutex::new(BlockSectorEntryInner::new(sector)),
            wait_queue: WaitQueue::new(),
            link: ListLinks::new(),
        }
    }

    fn lock_inner(&self) -> MutexGuard<'_, BlockSectorEntryInner> {
        self.inner.lock()
    }
}

pub struct BlockCache {
    sectors: Mutex<List<BlockSectorEntry>>,
    workqueue: Arc<Workqueue>,
    cache_count: AtomicUsize,
}

impl BlockCache {
    pub fn new() -> Self {
        Self {
            sectors: Mutex::new(List::new()),
            workqueue: Workqueue::create_workqueue(),
            cache_count: AtomicUsize::new(0),
        }
    }

    fn fetch_wq_work(arg: Option<ArcAny>) {
        let entry: &BlockSectorEntry = expect_downcast_ref(arg.as_ref());

        let mut inner = entry.inner.lock();
        inner.fetch_in_progress = true;
        let sector = inner.sector;
        inner.reset_from_disk(|entry_buffer| {
            DISK.get().unwrap().read_sector(sector, entry_buffer);
        });
        inner.fetch_in_progress = false;
        entry.wait_queue.unblock_all();
    }

    fn flush_wq_work(arg: Option<ArcAny>) {
        let entry: &BlockSectorEntry = expect_downcast_ref(arg.as_ref());

        let mut inner = entry.lock_inner();
        let sector = inner.sector;
        DISK.get()
            .unwrap()
            .write_sector(sector, inner.buffer.as_slice());
        inner.mark_flushed();
        entry.wait_queue.unblock_all();
    }

    fn fetch_block(
        &self,
        mut sectors: MutexGuard<'_, List<BlockSectorEntry>>,
        block_sector: u64,
    ) -> Arc<BlockSectorEntry> {
        let block_entry = UniqueArc::new(BlockSectorEntry::new(block_sector));

        let mut sector_cursor = sectors.cursor_mut();
        while sector_cursor
            .get()
            .is_some_and(|block| block.lock_inner().sector < block_sector)
        {
            let _ = sector_cursor.next();
        }

        self.cache_count.fetch_add(1, SeqCst);
        sector_cursor.insert_before(block_entry.into());
        let _ = sector_cursor.back();

        let block_entry = sector_cursor.get_arc().unwrap();
        block_entry.inner.lock().fetch_in_progress = true;

        block_entry.wait_queue.prepare_enqueue(|| {
            self.workqueue
                .enqueue_work(Self::fetch_wq_work, Some(block_entry.clone()));

            true
        });
        drop(sectors);
        block_entry.wait_queue.block();

        block_entry
    }

    fn wait_for_fetch(sector: &BlockSectorEntry) -> MutexGuard<'_, BlockSectorEntryInner> {
        loop {
            let waitqueue = &sector.wait_queue;
            let sector = sector.lock_inner();
            if waitqueue.prepare_enqueue(|| sector.fetch_in_progress) {
                drop(sector);
                waitqueue.block();
            } else {
                return sector;
            }
        }
    }

    fn read_within_block(&self, block: u64, offset: usize, buf: &mut [u8]) -> usize {
        let block_offset = offset % BLOCK_SIZE;
        let max_len = BLOCK_SIZE - block_offset;

        let len = buf.len().min(max_len);

        let sectors = self.sectors.lock();
        if let Some(sector) = sectors
            .cursor()
            .find(|entry| entry.lock_inner().sector == block)
        {
            let sector = Self::wait_for_fetch(sector);
            buf.copy_from_slice(&sector.buffer[block_offset..(block_offset + len)]);
        } else {
            let sector = self.fetch_block(sectors, block);
            let sector = sector.lock_inner();
            buf.copy_from_slice(&sector.buffer[block_offset..(block_offset + len)]);
        }

        len
    }

    pub fn read(&self, offset: usize, buf: &mut [u8]) {
        let mut buf = buf;
        let mut offset = offset;

        while !buf.is_empty() {
            let block_index = offset / BLOCK_SIZE;
            let block_offset = offset % BLOCK_SIZE;
            let to_read = buf.len().min(BLOCK_SIZE - block_offset);

            self.read_within_block(block_index as u64, block_offset, &mut buf[..to_read]);

            offset += to_read;
            buf = &mut buf[to_read..];
        }

        if self.cache_count.load(SeqCst) > (9 * MAX_CACHE_SIZE) / 10 {
            self.reclaim(None);
        }
    }

    fn write_within_block(&self, block: u64, offset: usize, buf: &[u8]) -> usize {
        let block_offset = offset % BLOCK_SIZE;
        let max_len = BLOCK_SIZE - block_offset;

        let len = buf.len().min(max_len);

        let sectors = self.sectors.lock();
        if let Some(sector) = sectors
            .cursor()
            .find(|entry| entry.lock_inner().sector == block)
        {
            let mut sector = sector.lock_inner();
            sector.modify_block(|sector_buffer| {
                sector_buffer[block_offset..(block_offset + len)].copy_from_slice(buf);
            });
        } else {
            let sector = self.fetch_block(sectors, block);
            let mut sector = sector.lock_inner();
            sector.modify_block(|sector_buffer| {
                sector_buffer[block_offset..(block_offset + len)].copy_from_slice(buf);
            });
        }

        len
    }

    pub fn write(&self, offset: usize, buf: &[u8]) {
        let mut buf = buf;
        let mut offset = offset;

        while !buf.is_empty() {
            let block_index = offset / BLOCK_SIZE;
            let block_offset = offset % BLOCK_SIZE;
            let to_write = buf.len().min(BLOCK_SIZE - block_offset);

            self.write_within_block(block_index as u64, block_offset, &buf[..to_write]);

            offset += to_write;
            buf = &buf[to_write..];
        }

        let mut sectors = self.sectors.lock();
        let mut cursor = sectors.cursor_mut();
        while let Some(block) = cursor.get() {
            if block.lock_inner().dirty && !block.lock_inner().is_flush_in_progress() {
                block.lock_inner().mark_flush_in_progress();
                self.workqueue
                    .enqueue_work(Self::flush_wq_work, Some(cursor.get_arc().unwrap()))
            }
            cursor.next();
        }

        if self.cache_count.load(SeqCst) > (9 * MAX_CACHE_SIZE) / 10 {
            self.reclaim(None);
        }
    }

    fn reclaim(&self, _arg: Option<ArcAny>) {
        let mut sectors = self.sectors.lock();
        let mut count = self.cache_count.load(SeqCst);
        let mut cursor = sectors.cursor_mut();

        while let Some(entry) = cursor.get()
            && count > (3 * MAX_CACHE_SIZE) / 4
        {
            let entry = entry.lock_inner();
            if !entry.dirty && !entry.fetch_in_progress {
                drop(entry);
                cursor.remove();
                count -= 1;
            } else {
                drop(entry);
                let _ = cursor.next();
            }
        }

        self.cache_count.store(count, SeqCst);
    }
}
