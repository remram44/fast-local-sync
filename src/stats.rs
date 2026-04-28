#[cfg(feature = "metrics")]
use lazy_static::lazy_static;

#[cfg(feature = "metrics")]
use prometheus::{
    Encoder, IntCounter, TextEncoder, gather,
    register_int_counter,
};

#[cfg(feature = "metrics")]
lazy_static! {
    static ref SCANNED_ENTRIES: IntCounter = register_int_counter!(
        "sync_scanned_entries",
        "Total number of entries scanned.",
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

pub fn add_listed_directory(count: usize) {
    #[cfg(feature = "metrics")]
    LISTED_DIRECTORIES.inc_by(count as u64);
}

pub fn add_errors(count: usize) {
    #[cfg(feature = "metrics")]
    ERRORS.inc_by(count as u64);
}
