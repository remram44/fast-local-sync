mod copy;
mod dir_scanner;
mod file_copier;
mod stats;

use pretty_env_logger;
use std::env::args_os;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::exit;

fn parse_num_option<N: std::str::FromStr>(opt: Option<OsString>, flag: &'static str) -> N {
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
    eprintln!("Invalid value for {}", flag);
    exit(2);
}

fn main() {
    // Initialize logging
    pretty_env_logger::init_timed();

    // Parse command line
    let mut source_target_pairs: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut next_source = None;
    let mut threads = None;
    let mut copy_queue = None;

    #[cfg(feature = "metrics")]
    let mut metrics_port = None;

    let mut args = args_os();
    args.next();
    let usage = format!(
        "\
Usage: fast-local-sync [options] SOURCE DESTINATION [SOURCE DESTINATION [...]]
Options:
    --threads NUM_THREADS
        Set the number of threads used for scanning and copying files (default: 8)
    --copy-queue SIZE
        Set the maximum number of files queued for copy (default: 4096){}
Environment variables:
    RUST_LOG
        Controls the logging level, for example \"info\"
        or \"fast_local_sync::copy=debug\"",
        {
            #[cfg(feature = "metrics")]
            {"
    --metrics PORT
        Expose the statistics in Prometheus format on HTTP PORT"}
            #[cfg(not(feature = "metrics"))]
            {""}
        },
    );
    while let Some(arg) = args.next() {
        if &arg == "--help" {
            println!("{}", usage);
            exit(0);
        } else if &arg == "--threads" {
            threads = Some(parse_num_option(args.next(), "--threads"));
        } else if &arg == "--copy-queue" {
            copy_queue = Some(parse_num_option(args.next(), "--copy-queue"));
        } else if &arg == "--metrics" {
            #[cfg(feature = "metrics")]
            {
                metrics_port = Some(parse_num_option(args.next(), "--metrics"));
            }
            #[cfg(not(feature = "metrics"))]
            {
                eprintln!("Option --metrics was not compiled in");
                exit(2);
            }
        } else {
            if let Some(next_source) = next_source.take() {
                source_target_pairs.push((next_source, arg.into()));
            } else {
                next_source = Some(arg.into());
            }
        }
    }

    if source_target_pairs.len() == 0 {
        eprintln!("Need at least one source and destination");
        exit(2);
    }

    if next_source.is_some() {
        eprintln!("Need as many sources as destinations");
        exit(2);
    }

    let threads = threads.unwrap_or(8);
    let copy_queue = copy_queue.unwrap_or(4096);

    for (_, target) in &source_target_pairs {
        if !target.exists() {
            eprintln!("Destination directory does not exist: {:?}", target);
            exit(1);
        }
    }

    let num_pairs = source_target_pairs.len();

    // Initialize statistics
    #[cfg(feature = "metrics")]
    if let Some(port) = metrics_port {
        stats::serve_prometheus(port, num_pairs);
    }

    // Create worker pools
    let file_copy_pool = file_copier::FileCopyPool::new(
        source_target_pairs.clone(),
        threads,
        copy_queue,
    );
    let dir_scan_pool = dir_scanner::DirScanPool::new(
        source_target_pairs,
        threads,
        file_copy_pool.clone(),
    );

    // Enqueue work
    for i in 0..num_pairs {
        dir_scan_pool.add(i, "".into());
    }

    // Wait until done
    dir_scan_pool.join();
    file_copy_pool.join();
}
