use crossbeam::channel::{Receiver, Sender, unbounded};
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{Metadata, read_dir, remove_dir_all, remove_file, symlink_metadata};
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use std::thread::{JoinHandle, sleep};
use tracing::{debug, error, info};

use crate::copy::copy_directory;
use crate::file_copier::FileCopyPool;
use crate::stats::Stats;

pub struct DirScanPool {
    source: PathBuf,
    target: PathBuf,
    queue_send: Sender<(PathBuf, bool)>,
    queue_recv: Receiver<(PathBuf, bool)>,
    enqueued: Arc<AtomicUsize>,
    file_copier: Arc<FileCopyPool>,
    threads: Mutex<Vec<(JoinHandle<()>, Arc<AtomicBool>)>>,
    stats: Arc<Stats>,
}

impl DirScanPool {
    pub fn new(
        source: &Path,
        target: &Path,
        num_threads: usize,
        file_copier: Arc<FileCopyPool>,
        stats: Arc<Stats>,
    ) -> Arc<DirScanPool> {
        // Create work queue
        let (send, recv) = unbounded();
        let enqueued = Arc::new(AtomicUsize::new(0));

        let pool = Arc::new(DirScanPool {
            source: source.to_owned(),
            target: target.to_owned(),
            queue_send: send,
            queue_recv: recv,
            enqueued,
            file_copier,
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

    pub fn add_no_check(&self, path: PathBuf) {
        debug!("scanner add_no_check {:?}", path);
        self.enqueued.fetch_add(1, Ordering::Relaxed);
        self.queue_send.send((path, false)).unwrap();
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

fn metadata_equal(a: &Metadata, b: &Metadata) -> bool {
    if a.file_type() != b.file_type() {
        return false;
    }
    if a.is_file() && a.len() != b.len() {
        return false;
    }
    if a.mode() != b.mode() {
        return false;
    }
    if a.uid() != b.uid() {
        return false;
    }
    if a.gid() != b.gid() {
        return false;
    }
    if a.modified().unwrap() != b.modified().unwrap() {
        return false;
    }
    return true;
}

fn dir_scan_thread(
    pool: Arc<DirScanPool>,
    stop_condition: Arc<AtomicBool>,
) {
    let pool = &*pool;
    let file_copier = &pool.file_copier;
    let stop_condition = &*stop_condition;
    let source = &pool.source;
    let target = &pool.target;

    let dir_scan = |dir_path: PathBuf, check_target: bool| {
        let mut seen_source_entries = HashSet::<OsString>::new();

        let source_dir = match read_dir(source.join(&dir_path)) {
            Ok(d) => d,
            Err(e) => {
                error!("Error reading directory: {}", e);
                pool.stats.add_errors(1);
                return;
            }
        };

        for source_entry in source_dir {
            let source_entry = match source_entry {
                Ok(s) => s,
                Err(e) => {
                    error!("Error reading directory entry: {}", e);
                    pool.stats.add_errors(1);
                    return;
                }
            };
            debug!("source path={:?} file_name={:?}", source_entry.path(), source_entry.file_name());
            let source_path = source_entry.path();
            let source_metadata = match source_entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    error!("Error reading source entry: {}", e);
                    pool.stats.add_errors(1);
                    return;
                }
            };
            let entry_path = dir_path.join(source_entry.file_name());
            seen_source_entries.insert(source_entry.file_name().to_owned());

            let target_path = target.join(&entry_path);
            debug!("target_path {:?}", target_path);

            let copy = || {
                if source_metadata.is_dir() {
                    if let Err(e) = copy_directory(&source_path, &target_path) {
                        error!("Error copying directory: {}", e);
                        pool.stats.add_errors(1);
                        return;
                    }

                    pool.add_no_check(entry_path.clone());
                } else {
                    file_copier.add(entry_path.clone());
                }
            };

            if !check_target {
                // Fast path: if the subtree doesn't exist on the target,
                // no need to check each entry
                copy();
            } else {
                match symlink_metadata(&target_path) {
                    Err(e) if e.kind() == ErrorKind::NotFound => {
                        // Target does not exist, copy
                        debug!("Target does not exist, copy {:?}", entry_path);
                        copy();
                    }
                    Err(e) => {
                        error!("Error reading target entry: {}", e);
                        pool.stats.add_errors(1);
                        continue;
                    }
                    Ok(target_metadata) => {
                        // Compare metadata
                        if source_metadata.file_type() != target_metadata.file_type() {
                            debug!("Different file type, removing target {:?}", target_path);
                            if target_metadata.is_dir() {
                                if let Err(e) = remove_dir_all(&target_path) {
                                    error!("Error removing target directory: {}", e);
                                    pool.stats.add_errors(1);
                                    continue;
                                }
                            } else {
                                if let Err(e) = remove_file(&target_path) {
                                    error!("Error removing target entry: {}", e);
                                    pool.stats.add_errors(1);
                                    continue;
                                }
                            }
                            // Target no longer exists, copy
                            copy();
                        } else if source_metadata.is_dir() {
                            if !metadata_equal(&source_metadata, &target_metadata) {
                                if let Err(e) = copy_directory(&source_path, &target_path) {
                                    error!("Error copying directory: {}", e);
                                    pool.stats.add_errors(1);
                                    continue;
                                }
                            }
                            // Recurse
                            pool.add(entry_path.clone());
                        } else if !metadata_equal(&source_metadata, &target_metadata) {
                            // Copy non-directory entry (file, link, ...)
                            file_copier.add(entry_path.clone());
                        } else {
                            pool.stats.add_skipped_entries(1);
                        }
                    }
                };
            }

            pool.stats.add_scanned_entries(1);
        }

        // Remove unseen entries in target
        let target_dir = match read_dir(target.join(&dir_path)) {
            Ok(d) => d,
            Err(e) => {
                error!("Error reading target directory: {}", e);
                pool.stats.add_errors(1);
                return;
            }
        };

        for target_entry in target_dir {
            let target_entry = match target_entry {
                Ok(s) => s,
                Err(e) => {
                    error!("Error reading target directory entry: {}", e);
                    pool.stats.add_errors(1);
                    return;
                }
            };
            if !seen_source_entries.contains(&target_entry.file_name()) {
                let target_metadata = match target_entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        error!("Error reading target directory entry: {}", e);
                        pool.stats.add_errors(1);
                        return;
                    }
                };

                if target_metadata.is_dir() {
                    debug!("Removing directory, not in source: {:?}", target_entry.path());
                    if let Err(e) = remove_dir_all(target_entry.path()) {
                        error!("Error removing target directory: {}", e);
                        pool.stats.add_errors(1);
                        continue;
                    }
                } else {
                    debug!("Removing file, not in source: {:?}", target_entry.path());
                    if let Err(e) = remove_file(target_entry.path()) {
                        error!("Error removing target entry: {}", e);
                        pool.stats.add_errors(1);
                        continue;
                    }
                }
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
        dir_scan(path, check_target);

        pool.enqueued.fetch_sub(1, Ordering::Relaxed);
    }
}
