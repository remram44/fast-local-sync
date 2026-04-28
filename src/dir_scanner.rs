use crossbeam::channel::{Receiver, Sender, unbounded};
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use std::thread::{JoinHandle, sleep};
use tracing::{debug, error, info};

use crate::stats;

pub struct DirScanPool {
    source: PathBuf,
    queue_send: Sender<(PathBuf, bool)>,
    queue_recv: Receiver<(PathBuf, bool)>,
    enqueued: Arc<AtomicUsize>,
    threads: Mutex<Vec<(JoinHandle<()>, Arc<AtomicBool>)>>,
}

impl DirScanPool {
    pub fn new(
        source: &Path,
        num_threads: usize,
    ) -> Arc<DirScanPool> {
        // Create work queue
        let (send, recv) = unbounded();
        let enqueued = Arc::new(AtomicUsize::new(0));

        let pool = Arc::new(DirScanPool {
            source: source.to_owned(),
            queue_send: send,
            queue_recv: recv,
            enqueued,
            threads: Mutex::new(Vec::new()),
        });

        // Start threads
        {
            let mut threads = pool.threads.lock().unwrap();
            for _ in 0..num_threads {
                let pool2 = pool.clone();
                let cond = Arc::new(AtomicBool::new(false));
                let cond2 = cond.clone();
                let thread = std::thread::spawn(move || {
                    dir_scan_thread(
                        pool2,
                        cond2,
                    )
                });
                threads.push((thread, cond));
            }
            info!("Created {} dir scanner threads", num_threads);
        }

        pool
    }

    pub fn add(&self, path: PathBuf) {
        debug!("scanner add {:?}", path);
        self.enqueued.fetch_add(1, Ordering::Relaxed);
        self.queue_send.send((path, true)).unwrap();
    }

    pub fn join(&self) {
        let enqueued = &*self.enqueued;
        loop {
            debug!("dir scanner enqueued {}", enqueued.load(Ordering::Relaxed));
            if enqueued.load(Ordering::Relaxed) == 0 {
                return;
            }
            sleep(Duration::from_secs(2));
        }
    }
}

fn dir_scan_thread(
    pool: Arc<DirScanPool>,
    stop_condition: Arc<AtomicBool>,
) {
    let pool = &*pool;
    let stop_condition = &*stop_condition;
    let source = &pool.source;

    let mut scanned_entries = 0;
    let mut listed_directories = 0;

    let dir_scan = |dir_path: PathBuf, scanned_entries: &mut u32| {
        let source_dir = match read_dir(source.join(&dir_path)) {
            Ok(d) => d,
            Err(e) => {
                error!("Error reading directory: {}", e);
                stats::add_errors(1);
                return;
            }
        };

        for source_entry in source_dir {
            let source_entry = match source_entry {
                Ok(s) => s,
                Err(e) => {
                    error!("Error reading directory entry: {}", e);
                    stats::add_errors(1);
                    return;
                }
            };
            debug!("source path={:?} file_name={:?}", source_entry.path(), source_entry.file_name());
            let source_path = source_entry.path();
            let source_metadata = match source_entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    error!("Error reading source entry: {}", e);
                    stats::add_errors(1);
                    return;
                }
            };

            let entry_path = dir_path.join(source_entry.file_name());
            if source_metadata.is_dir() {
                pool.add(entry_path.clone());
            }

            std::hint::black_box((source_path, source_metadata));

            *scanned_entries += 1;
            if *scanned_entries == 1000 {
                stats::add_scanned_entries(1000);
                *scanned_entries = 0;
            }
        }
    };

    loop {
        let (path, check_target) = match pool.queue_recv.recv_timeout(Duration::from_secs(5)) {
            Ok(p) => p,
            Err(_) => {
                // Check if we should stop
                if stop_condition.load(Ordering::Relaxed) {
                    debug!("Stop condition true, exiting thread");
                    return;
                }
                continue;
            }
        };

        debug!("Scanning {:?}, check_target={}", path, check_target);
        dir_scan(path, &mut scanned_entries);
        listed_directories += 1;
        if listed_directories == 1000 {
            stats::add_listed_directory(1000);
            listed_directories = 0;
        }

        pool.enqueued.fetch_sub(1, Ordering::Relaxed);
    }
}
