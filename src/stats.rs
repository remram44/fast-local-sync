use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

pub struct Stats {
    scanned_entries: AtomicUsize,
    skipped_entries: AtomicUsize,
    queued_copy_entries: AtomicUsize,
    copied_entries: AtomicUsize,
    copied_bytes: AtomicU64,
    removed_entries: AtomicUsize,
    removed_bytes: AtomicU64,
    errors: AtomicUsize,
}

impl Stats {
    pub fn new() -> Arc<Stats> {
        let stats = Arc::new(Stats {
            scanned_entries: AtomicUsize::new(0),
            skipped_entries: AtomicUsize::new(0),
            queued_copy_entries: AtomicUsize::new(0),
            copied_entries: AtomicUsize::new(0),
            copied_bytes: AtomicU64::new(0),
            removed_entries: AtomicUsize::new(0),
            removed_bytes: AtomicU64::new(0),
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

    #[cfg(feature = "metrics")]
    pub fn serve_prometheus(self: &Arc<Self>, port: u16) {
        use tokio::runtime::Builder;
        use tracing::info;
        use warp::Filter;

        let stats = self.clone();

        std::thread::spawn(move || {
            info!("Starting Prometheus HTTP server on port {}", port);

            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
                let routes = warp::path("metrics").map(move || {
                    use std::io::Write;

                    let mut buffer = vec![];

                    write!(
                        &mut buffer,
                        "# HELP sync_scanned_entries Total number of entries scanned.\n\
                        # TYPE sync_scanned_entries counter\n\
                        sync_scanned_entries {}\n",
                        stats.scanned_entries.load(Ordering::Relaxed),
                    ).unwrap();

                    write!(
                        &mut buffer,
                        "# HELP sync_skipped_entries Total number of entries skipped because they were up-to-date.\n\
                        # TYPE sync_skipped_entries counter\n\
                        sync_skipped_entries {}\n",
                        stats.skipped_entries.load(Ordering::Relaxed),
                    ).unwrap();

                    write!(
                        &mut buffer,
                        "# HELP sync_queued_copy_entries Total number of entries added to the queue for copy.\n\
                        # TYPE sync_queued_copy_entries counter\n\
                        sync_queued_copy_entries {}\n",
                        stats.queued_copy_entries.load(Ordering::Relaxed),
                    ).unwrap();

                    write!(
                        &mut buffer,
                        "# HELP sync_copied_entries Total number of files copied.\n\
                        # TYPE sync_copied_entries counter\n\
                        sync_copied_entries {}\n",
                        stats.copied_entries.load(Ordering::Relaxed),
                    ).unwrap();

                    write!(
                        &mut buffer,
                        "# HELP sync_copied_bytes Total size of files copied.\n\
                        # TYPE sync_copied_bytes counter\n\
                        sync_copied_bytes {}\n",
                        stats.copied_bytes.load(Ordering::Relaxed),
                    ).unwrap();

                    write!(
                        &mut buffer,
                        "# HELP sync_removed_entries Total number of entries deleted.\n\
                        # TYPE sync_removed_entries counter\n\
                        sync_removed_entries {}\n",
                        stats.removed_entries.load(Ordering::Relaxed),
                    ).unwrap();

                    write!(
                        &mut buffer,
                        "# HELP sync_removed_bytes Total size of files deleted.\n\
                        # TYPE sync_removed_bytes counter\n\
                        sync_removed_bytes {}\n",
                        stats.removed_bytes.load(Ordering::Relaxed),
                    ).unwrap();

                    write!(
                        &mut buffer,
                        "# HELP sync_errors Total number of errors during this sync operation.\n\
                        # TYPE sync_errors counter\n\
                        sync_errors {}\n",
                        stats.errors.load(Ordering::Relaxed),
                    ).unwrap();

                    buffer
                });
                warp::serve(routes).run(addr).await;
            });
        });
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
                     REMOVED     \
                     ERRORS"
                );
            }
            i += 1;
            println!(
                "{:>10}  {:>10}  {:>10}  {:>10}  {:>10}  {:>10}",
                self.scanned_entries.load(Ordering::Relaxed),
                self.skipped_entries.load(Ordering::Relaxed),
                self.queued_copy_entries.load(Ordering::Relaxed),
                self.copied_entries.load(Ordering::Relaxed),
                self.removed_entries.load(Ordering::Relaxed),
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

    pub fn add_copied(&self, count: usize, bytes: u64) {
        self.copied_entries.fetch_add(count, Ordering::Relaxed);
        if bytes != 0 {
            self.copied_bytes.fetch_add(bytes, Ordering::Relaxed);
        }
    }

    pub fn add_removed(&self, count: usize, bytes: u64) {
        self.removed_entries.fetch_add(count, Ordering::Relaxed);
        if bytes != 0 {
            self.removed_bytes.fetch_add(bytes, Ordering::Relaxed);
        }
    }

    pub fn add_errors(&self, count: usize) {
        self.errors.fetch_add(count, Ordering::Relaxed);
    }
}
