use crossbeam::channel::{Receiver, Sender, bounded};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::sleep;
use std::time::Duration;
use std::thread::JoinHandle;
use tracing::{debug, error, info};

use crate::copy::copy_file;
use crate::stats::Stats;

pub struct FileCopyPool {
    source: PathBuf,
    target: PathBuf,
    queue_send: Sender<PathBuf>,
    queue_recv: Receiver<PathBuf>,
    enqueued: Arc<AtomicUsize>,
    threads: Mutex<Vec<(JoinHandle<()>, Arc<AtomicBool>)>>,
    stats: Arc<Stats>,
}

impl FileCopyPool {
    pub fn new(
        source: &Path,
        target: &Path,
        num_threads: usize,
        stats: Arc<Stats>,
    ) -> Arc<FileCopyPool> {
        // Create work queue
        let (send, recv) = bounded(4096);
        let enqueued = Arc::new(AtomicUsize::new(0));

        let pool = Arc::new(FileCopyPool {
            source: source.to_owned(),
            target: target.to_owned(),
            queue_send: send,
            queue_recv: recv,
            enqueued,
            threads: Mutex::new(Vec::new()),
            stats,
        });

        // Start threads
        {
            let mut threads = pool.threads.lock().unwrap();
            for _ in 0..num_threads {
                let pool2 = pool.clone();
                let cond = Arc::new(AtomicBool::new(false));
                let cond2 = cond.clone();
                let thread = std::thread::spawn(move || {
                    file_copy_thread(
                        pool2,
                        cond2,
                    )
                });
                threads.push((thread, cond));
            }
            info!("Created {} file copy threads", num_threads);
        }

        pool
    }

    pub fn add(&self, path: PathBuf) {
        debug!("copier add {:?}", path);
        self.enqueued.fetch_add(1, Ordering::Relaxed);
        self.stats.add_queued_copy_entries(1);
        self.queue_send.send(path).unwrap();
    }

    pub fn join(&self) {
        let enqueued = &*self.enqueued;
        loop {
            debug!("file copier enqueued {}", enqueued.load(Ordering::Relaxed));
            if enqueued.load(Ordering::Relaxed) == 0 {
                return;
            }
            sleep(Duration::from_secs(2));
        }
    }
}

fn file_copy_thread(
    pool: Arc<FileCopyPool>,
    stop_condition: Arc<AtomicBool>,
) {
    let pool = &*pool;

    loop {
        let path = match pool.queue_recv.recv_timeout(Duration::from_secs(5)) {
            Ok(p) => p,
            Err(_) => {
                // Check if we should stop
                if stop_condition.load(Ordering::Relaxed) {
                    return;
                }
                continue;
            }
        };

        let source_path = pool.source.join(&path);
        let target_path = pool.target.join(&path);

        debug!("copy {:?} -> {:?}", source_path, target_path);

        if let Err(e) = copy_file(&source_path, &target_path) {
            error!("Error copying file: {}", e);
        }

        pool.stats.add_copied_entries(1);

        pool.enqueued.fetch_sub(1, Ordering::Relaxed);
    }
}
