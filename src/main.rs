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
    eprintln!("Invalid value for --entries");
    exit(2);
}

fn main() {
    // Initialize logging
    pretty_env_logger::init();

    // Parse command line
    let mut source = None;
    let mut target = None;
    let mut threads = None;
    let mut print_stats = false;

    #[cfg(feature = "metrics")]
    let mut metrics_port = None;

    let mut args = args_os();
    args.next();
    let usage = format!(
        "Usage: fast-local-sync [--threads NUM_THREADS] [--print-stats] {}SOURCE DESTINATION",
        {
            #[cfg(feature = "metrics")]
            {"[--metrics PORT] "}
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
        } else if &arg == "--print-stats" {
            print_stats = true;
        } else {

            if source.is_none() {
                source = Some(arg);
            } else if target.is_none() {
                target = Some(arg);
            } else {
                eprintln!("Too many arguments");
                eprintln!("{}", usage);
                exit(2);
            }
        }
    }

    let threads = threads.unwrap_or(8);
    let source: PathBuf = match source {
        Some(s) => s.into(),
        None => {
            eprintln!("Missing source");
            eprintln!("{}", usage);
            exit(2);
        }
    };
    let target: PathBuf = match target {
        Some(s) => s.into(),
        None => {
            eprintln!("Missing target");
            eprintln!("{}", usage);
            exit(2);
        }
    };

    if !target.exists() {
        eprintln!("Destination directory does not exist!");
        exit(1);
    }

    // Initialize statistics
    let stats = stats::Stats::new();
    if print_stats {
        stats.start_print_loop();
    }
    #[cfg(feature = "metrics")]
    if let Some(port) = metrics_port {
        stats.serve_prometheus(port);
    }

    // Create worker pools
    let file_copy_pool = file_copier::FileCopyPool::new(
        source.as_path(),
        target.as_path(),
        threads,
        stats.clone(),
    );
    let dir_scan_pool = dir_scanner::DirScanPool::new(
        source.as_path(),
        target.as_path(),
        threads,
        file_copy_pool.clone(),
        stats.clone(),
    );

    // Enqueue work
    dir_scan_pool.add("".into());

    // Wait until done
    dir_scan_pool.join();
    file_copy_pool.join();
}
