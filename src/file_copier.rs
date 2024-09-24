use crossbeam::channel::{Receiver, Sender, bounded};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::sleep;
use std::time::Duration;
use std::thread::JoinHandle;

use crate::stats::Stats;

pub struct FileCopyPool {
    queue_send: Sender<PathBuf>,
    queue_recv: Receiver<PathBuf>,
    enqueued: Arc<AtomicUsize>,
    threads: Vec<(JoinHandle<()>, Arc<AtomicBool>)>,
}

impl FileCopyPool {
    pub fn new(
        source: &Path,
        target: &Path,
        num_threads: usize,
        stats: Arc<Stats>,
    ) -> FileCopyPool {
        // Create work queue
        let (send, recv) = bounded(4096);
        let enqueued = Arc::new(AtomicUsize::new(0));

        // Start threads
        let mut threads = Vec::new();
        for _ in 0..num_threads {
            let source = source.to_owned();
            let target = target.to_owned();
            let recv2 = recv.clone();
            let enqueued = enqueued.clone();
            let stats = stats.clone();
            let cond = Arc::new(AtomicBool::new(false));
            let cond2 = cond.clone();
            let thread = std::thread::spawn(move || {
                file_copy_thread(
                    source,
                    target,
                    recv2,
                    enqueued,
                    stats,
                    cond2,
                )
            });
            threads.push((thread, cond));
        }

        FileCopyPool {
            queue_send: send,
            queue_recv: recv,
            enqueued,
            threads,
        }
    }

    pub fn add(&self, path: PathBuf) {
        self.enqueued.fetch_add(1, Ordering::Relaxed);
        self.queue_send.send(path).unwrap();
    }

    pub fn join(&self) {
        let enqueued = &*self.enqueued;
        loop {
            if enqueued.load(Ordering::Relaxed) > 0 {
                sleep(Duration::from_secs(5));
            }
        }
    }
}

fn file_copy_thread(
    source: PathBuf,
    target: PathBuf,
    queue: Receiver<PathBuf>,
    enqueued: Arc<AtomicUsize>,
    stats: Arc<Stats>,
    stop_condition: Arc<AtomicBool>,
) {
    loop {
        let path = match queue.recv_timeout(Duration::from_secs(5)) {
            Ok(p) => p,
            Err(_) => {
                // Check if we should stop
                if stop_condition.load(Ordering::Relaxed) {
                    return;
                }
                continue;
            }
        };

        // TODO: Copy

        enqueued.fetch_sub(1, Ordering::Relaxed);
    }
}
