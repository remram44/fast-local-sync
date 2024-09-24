use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

pub struct Stats {
    total_entries: Option<usize>,
    total_size: Option<usize>,
    scanned_entries: AtomicUsize,
    skipped_entries: AtomicUsize,
    queued_copy_entries: AtomicUsize,
    copied_entries: AtomicUsize,
}

impl Stats {
    pub fn new(total_entries: Option<usize>, total_size: Option<usize>) -> Arc<Stats> {
        let stats = Arc::new(Stats {
            total_entries,
            total_size,
            scanned_entries: AtomicUsize::new(0),
            skipped_entries: AtomicUsize::new(0),
            queued_copy_entries: AtomicUsize::new(0),
            copied_entries: AtomicUsize::new(0),
        });

        {
            let stats = stats.clone();
            thread::spawn(move || {
                let stats = &*stats;
                stats.print_thread();
            });
        }

        stats
    }

    fn print_thread(&self) {
        let mut i = 0;

        loop {
            thread::sleep(Duration::from_secs(10));

            i += 1;
            if i >= 30 {
                i = 1;
                eprintln!(
                    "SCANNED   \
                     SKIPPED   \
                     QUEUED    \
                     COPIED"
                );
            }
            eprintln!(
                "{:>9} {:>9} {:>9} {:>9}",
                self.scanned_entries(),
                self.skipped_entries(),
                self.queued_copy_entries(),
                self.copied_entries(),
            )
        }
    }

    pub fn add_scanned_entries(&self, count: usize) {
        self.scanned_entries.fetch_add(count, Ordering::Relaxed);
    }

    pub fn scanned_entries(&self) -> usize {
        self.scanned_entries.load(Ordering::Relaxed)
    }

    pub fn add_skipped_entries(&self, count: usize) {
        self.skipped_entries.fetch_add(count, Ordering::Relaxed);
    }

    pub fn skipped_entries(&self) -> usize {
        self.skipped_entries.load(Ordering::Relaxed)
    }

    pub fn add_queued_copy_entries(&self, count: usize) {
        self.queued_copy_entries.fetch_add(count, Ordering::Relaxed);
    }

    pub fn queued_copy_entries(&self) -> usize {
        self.queued_copy_entries.load(Ordering::Relaxed)
    }

    pub fn add_copied_entries(&self, count: usize) {
        self.copied_entries.fetch_add(count, Ordering::Relaxed);
    }

    pub fn copied_entries(&self) -> usize {
        self.copied_entries.load(Ordering::Relaxed)
    }
}
