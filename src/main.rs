mod dir_scanner;
mod file_copier;

use std::env::args_os;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;

fn parse_num_option(opt: Option<OsString>, flag: &'static str) -> usize {
    let opt = match opt {
        Some(o) => o,
        None => {
            eprintln!("Missing value for {}", flag);
            exit(2);
        }
    };
    if let Some(opt) = opt.to_str() {
        if let Ok(opt) = opt.parse() {
            return opt;
        }
    }
    eprintln!("Invalid value for --entries");
    exit(2);
}

fn main() {
    // Parse command line
    let mut entries = None;
    let mut size = None;
    let mut source = None;
    let mut target = None;
    let mut threads = None;

    let mut args = args_os();
    while let Some(arg) = args.next() {
        if &arg == "--help" {
            println!("Usage: cephfssync [--entries TOTAL_ENTRIES] [--size TOTAL_SIZE] [--threads NUM_THREADS] SOURCE DESTINATION");
            exit(0);
        } else if &arg == "--entries" {
            entries = Some(parse_num_option(args.next(), "--entries"));
        } else if &arg == "--size" {
            size = Some(parse_num_option(args.next(), "--size"));
        } else if &arg == "--threads" {
            threads = Some(parse_num_option(args.next(), "--threads"));
        } else {
            if source.is_none() {
                source = Some(arg);
            } else if target.is_none() {
                target = Some(arg);
            } else {
                eprintln!("Too many arguments");
                exit(2);
            }
        }
    }

    let threads = threads.unwrap_or(8);
    let source: PathBuf = match source {
        Some(s) => s.into(),
        None => {
            eprintln!("Missing source");
            exit(2);
        }
    };
    let target: PathBuf = match target {
        Some(s) => s.into(),
        None => {
            eprintln!("Missing target");
            exit(2);
        }
    };

    // Create worker pools
    let file_copy_pool = Arc::new(file_copier::FileCopyPool::new(
        source.as_path(),
        target.as_path(),
        threads,
    ));
    let dir_scan_pool = dir_scanner::DirScanPool::new(
        source.as_path(),
        target.as_path(),
        threads,
        file_copy_pool.clone(),
    );

    // Enqueue work
    dir_scan_pool.add("/".into());

    dir_scan_pool.join();

    file_copy_pool.join();
}
