use std::{collections::HashMap, env, path::Path, time::Duration};

use axum::{Router, routing::get};
use proberv2::{actor::Actor, config::Config, metrics, setup_nftables::setup_nftables};
use tokio::{
    fs,
    signal::unix::{self, SignalKind},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("prober: program starting");

    let config_path = env::var("CONFIG_PATH")?;
    let config_path = Path::new(&config_path);
    let config = fs::read_to_string(config_path).await?;
    let mut config: Config = serde_json::from_str(&config)?;

    let node_name = env::var("NODENAME")?;
    config.kubernetes.node_name = node_name;

    // inisialisasi metrics registry
    metrics::init();

    let token = CancellationToken::new();
    let child_token = token.clone();

    // menjalankan HTTP server untuk endpoint /metrics Prometheus
    if config.metrics.enabled {
        let metrics_token = token.clone();
        let listen_addr = config.metrics.listen_addr.clone();
        tokio::spawn(async move {
            let app = Router::new()
                .route("/metrics", get(|| async { metrics::gather() }))
                .route("/healthz", get(|| async { "OK" }));

            let listener = match tokio::net::TcpListener::bind(&listen_addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!("metrics: gagal bind ke {listen_addr}: {e}");
                    return;
                }
            };
            info!("metrics: server listening on {listen_addr}");

            axum::serve(listener, app)
                .with_graceful_shutdown(async move { metrics_token.cancelled().await })
                .await
                .unwrap_or_else(|e| error!("metrics: server error: {e}"));
        });
    }

    let mut actor = Actor {
        config: config.clone(),
        datapoint_by_nodename: HashMap::new(),
        service_by_nodeport: HashMap::new(),
    };
    setup_nftables(&config).await?;

    tokio::spawn(async move { actor.dispatch(child_token).await });

    let mut sigint = unix::signal(SignalKind::interrupt())?;
    let mut sigterm = unix::signal(SignalKind::terminate())?;

    tokio::select! {
        _ = sigint.recv() => {},
        _ = sigterm.recv() => {},
    }
    info!("main: received shutdown signal, terminating...");
    token.cancel();
    tokio::time::sleep(Duration::from_secs(config.shutdown_timeout.into())).await;

    Ok(())
}
