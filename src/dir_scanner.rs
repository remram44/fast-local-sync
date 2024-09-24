use crossbeam::channel::{Receiver, Sender, bounded};
use std::collections::HashSet;
use std::fs::{metadata, read_dir, remove_dir_all, remove_file};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use std::thread::{JoinHandle, sleep};

use crate::file_copier::FileCopyPool;
use crate::stats::Stats;

pub struct DirScanPool {
    queue_send: Sender<(PathBuf, bool)>,
    queue_recv: Receiver<(PathBuf, bool)>,
    enqueued: Arc<AtomicUsize>,
    file_copier: Arc<FileCopyPool>,
    threads: Vec<(JoinHandle<()>, Arc<AtomicBool>)>,
}

impl DirScanPool {
    pub fn new(
        source: &Path,
        target: &Path,
        num_threads: usize,
        file_copier: Arc<FileCopyPool>,
        stats: Arc<Stats>,
    ) -> DirScanPool {
        // Create work queue
        let (send, recv) = bounded(4096);
        let enqueued = Arc::new(AtomicUsize::new(0));

        // Start threads
        let mut threads = Vec::new();
        for _ in 0..num_threads {
            let source = source.to_owned();
            let target = target.to_owned();
            let recv2 = recv.clone();
            let send2 = send.clone();
            let enqueued = enqueued.clone();
            let file_copier = file_copier.clone();
            let stats = stats.clone();
            let cond = Arc::new(AtomicBool::new(false));
            let cond2 = cond.clone();
            let thread = std::thread::spawn(move || {
                dir_scan_thread(
                    source,
                    target,
                    recv2,
                    send2,
                    enqueued,
                    file_copier,
                    stats,
                    cond2,
                )
            });
            threads.push((thread, cond));
        }

        DirScanPool {
            queue_send: send,
            queue_recv: recv,
            enqueued,
            file_copier,
            threads,
        }
    }

    pub fn add(&self, path: PathBuf) {
        self.enqueued.fetch_add(1, Ordering::Relaxed);
        self.queue_send.send((path, true)).unwrap();
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

fn dir_scan_thread(
    source: PathBuf,
    target: PathBuf,
    queue: Receiver<(PathBuf, bool)>,
    queue_send: Sender<(PathBuf, bool)>,
    enqueued: Arc<AtomicUsize>,
    file_copier: Arc<FileCopyPool>,
    stats: Arc<Stats>,
    stop_condition: Arc<AtomicBool>,
) {
    let enqueued = &*enqueued;
    let file_copier = &*file_copier;
    let stop_condition = &*stop_condition;

    let dir_scan = |path: PathBuf, check_target: bool| {
        let mut seen_source_entries = HashSet::<PathBuf>::new();

        let source_dir = match read_dir(source.join(&path)) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Error reading directory: {}", e);
                return;
            }
        };

        for source_entry in source_dir {
            let source_entry = match source_entry {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading directory entry: {}", e);
                    return;
                }
            };
            let source_path = source_entry.path();
            let source_metadata = match source_entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error reading source entry: {}", e);
                    return;
                }
            };

            let target_path = target.join(&path);

            let copy = || {
                if source_metadata.file_type().is_dir() {
                    enqueued.fetch_add(1, Ordering::Relaxed);
                    queue_send.send((path.clone(), false));
                } else {
                    file_copier.add(path.clone());
                }
            };

            if check_target {
                match metadata(&target_path) {
                    Err(e) if e.kind() == ErrorKind::NotFound => {
                        // Target does not exist, copy
                        copy();
                    }
                    Err(e) => {
                        eprintln!("Error reading target entry: {}", e);
                        return;
                    }
                    Ok(target_metadata) => {
                        // Compare metadata
                        if source_metadata.file_type() != target_metadata.file_type() {
                            if target_metadata.file_type().is_dir() {
                                if let Err(e) = remove_dir_all(&target_path) {
                                    eprintln!("Error removing target directory: {}", e);
                                    return;
                                }
                            } else {
                                if let Err(e) = remove_file(&target_path) {
                                    eprintln!("Error removing target entry: {}", e);
                                    return;
                                }
                            }
                            copy();
                        } else if source_metadata.file_type().is_dir() {
                            enqueued.fetch_add(1, Ordering::Relaxed);
                            queue_send.send((path.clone(), true));
                        }
                    }
                };
            } else {
                copy();
            }

            seen_source_entries.insert(source_path);
        }

        // TODO: Remove unseen entries in target
    };

    loop {
        let (path, check_target) = match queue.recv_timeout(Duration::from_secs(5)) {
            Ok(p) => p,
            Err(_) => {
                // Check if we should stop
                if stop_condition.load(Ordering::Relaxed) {
                    return;
                }
                continue;
            }
        };

        dir_scan(path, check_target);

        enqueued.fetch_sub(1, Ordering::Relaxed);
    }
}
