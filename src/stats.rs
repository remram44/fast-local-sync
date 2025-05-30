#[cfg(feature = "metrics")]
use lazy_static::lazy_static;

#[cfg(feature = "metrics")]
use prometheus::{
    Encoder, IntCounter, IntGauge, TextEncoder, gather,
    register_int_counter, register_int_gauge,
};

#[cfg(feature = "metrics")]
lazy_static! {
    static ref SCANNED_ENTRIES: IntCounter = register_int_counter!(
        "sync_scanned_entries",
        "Total number of entries scanned.",
    ).unwrap();
    static ref SKIPPED_ENTRIES: IntCounter = register_int_counter!(
        "sync_skipped_entries",
        "Total number of entries skipped because they were up-to-date.",
    ).unwrap();
    static ref SKIPPED_BYTES: IntCounter = register_int_counter!(
        "sync_skipped_bytes",
        "Total size of files skipped because they were up-to-date.",
    ).unwrap();
    static ref QUEUED_COPY_ENTRIES: IntGauge = register_int_gauge!(
        "sync_copy_entries_in_queue",
        "Number of entries currently in the queue for copy.",
    ).unwrap();
    static ref COPIED_ENTRIES: IntCounter = register_int_counter!(
        "sync_copied_entries",
        "Total number of files copied.",
    ).unwrap();
    static ref COPIED_BYTES: IntCounter = register_int_counter!(
        "sync_copied_bytes",
        "Total size of files copied.",
    ).unwrap();
    static ref REMOVED_ENTRIES: IntCounter = register_int_counter!(
        "sync_removed_entries",
        "Total number of entries deleted.",
    ).unwrap();
    static ref REMOVED_BYTES: IntCounter = register_int_counter!(
        "sync_removed_bytes",
        "Total size of files deleted.",
    ).unwrap();
    static ref LISTED_DIRECTORIES: IntCounter = register_int_counter!(
        "sync_listed_directories",
        "Total number of directories listed.",
    ).unwrap();
    static ref ERRORS: IntCounter = register_int_counter!(
        "sync_errors",
        "Total number of errors during this sync operation.",
    ).unwrap();
}

#[cfg(feature = "metrics")]
pub fn serve_prometheus(port: u16) {
    use tokio::runtime::Builder;
    use tracing::info;
    use warp::Filter;
    use warp::http::Response;

    std::thread::spawn(move || {
        info!("Starting Prometheus HTTP server on port {}", port);

        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
            let routes = warp::path("metrics").map(move || {
                let mut buffer = vec![];
                let encoder = TextEncoder::new();
                let metric_families = gather();
                encoder.encode(&metric_families, &mut buffer).unwrap();

                Response::builder()
                    .header("Content-type", "text/plain")
                    .body(buffer)
            });
            warp::serve(routes).run(addr).await;
        });
    });
}

pub fn add_scanned_entries(count: usize) {
    #[cfg(feature = "metrics")]
    SCANNED_ENTRIES.inc_by(count as u64);
}

pub fn add_skipped(count: usize, bytes: u64) {
    #[cfg(feature = "metrics")]
    {
        SKIPPED_ENTRIES.inc_by(count as u64);
        if bytes != 0 {
            SKIPPED_BYTES.inc_by(bytes);
        }
    }
}

pub fn add_queued_copy_entries(count: usize) {
    #[cfg(feature = "metrics")]
    QUEUED_COPY_ENTRIES.add(count as i64);
}

pub fn sub_queued_copy_entries(count: usize) {
    #[cfg(feature = "metrics")]
    QUEUED_COPY_ENTRIES.sub(count as i64);
}

pub fn add_copied(count: usize, bytes: u64) {
    #[cfg(feature = "metrics")]
    {
        COPIED_ENTRIES.inc_by(count as u64);
        if bytes != 0 {
            COPIED_BYTES.inc_by(bytes);
        }
    }
}

pub fn add_removed(count: usize, bytes: u64) {
    #[cfg(feature = "metrics")]
    {
        REMOVED_ENTRIES.inc_by(count as u64);
        if bytes != 0 {
            REMOVED_BYTES.inc_by(bytes);
        }
    }
}

pub fn add_listed_directory(count: usize) {
    #[cfg(feature = "metrics")]
    LISTED_DIRECTORIES.inc_by(count as u64);
}

pub fn add_errors(count: usize) {
    #[cfg(feature = "metrics")]
    ERRORS.inc_by(count as u64);
}
