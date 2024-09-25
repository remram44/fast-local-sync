use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

pub struct Stats {
    scanned_entries: AtomicUsize,
    skipped_entries: AtomicUsize,
    queued_copy_entries: AtomicUsize,
    copied_entries: AtomicUsize,
    errors: AtomicUsize,
}

impl Stats {
    pub fn new() -> Arc<Stats> {
        let stats = Arc::new(Stats {
            scanned_entries: AtomicUsize::new(0),
            skipped_entries: AtomicUsize::new(0),
            queued_copy_entries: AtomicUsize::new(0),
            copied_entries: AtomicUsize::new(0),
            errors: AtomicUsize::new(0),
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

            if i % 30 == 0 {
                i = 0;
                println!(
                    "SCANNED     \
                     SKIPPED     \
                     QUEUED      \
                     COPIED      \
                     ERRORS"
                );
            }
            i += 1;
            println!(
                "{:>10}  {:>10}  {:>10}  {:>10}  {:>10}",
                self.scanned_entries.load(Ordering::Relaxed),
                self.skipped_entries.load(Ordering::Relaxed),
                self.queued_copy_entries.load(Ordering::Relaxed),
                self.copied_entries.load(Ordering::Relaxed),
                self.errors.load(Ordering::Relaxed),
            )
        }
    }

    pub fn add_scanned_entries(&self, count: usize) {
        self.scanned_entries.fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_skipped_entries(&self, count: usize) {
        self.skipped_entries.fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_queued_copy_entries(&self, count: usize) {
        self.queued_copy_entries.fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_copied_entries(&self, count: usize) {
        self.copied_entries.fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_errors(&self, count: usize) {
        self.errors.fetch_add(count, Ordering::Relaxed);
    }
}
