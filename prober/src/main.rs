use kube::Client;
use prober::{
    cpu_watcher::CpuCollector,
    latency_prober::{LatencyProber, ProbeResult},
    nftables_balancer::NftablesBalancer,
    nftables_watcher::NftablesWatcher,
};
use std::{collections::HashMap, env, sync::Arc, time::Duration};

use axum::{Router, routing};
use futures::lock::Mutex;
use tokio::{
    net::TcpListener,
    sync::broadcast::{self, Sender},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let delay: u64 = env::var("INTERVAL_IN_SECONDS")
        .unwrap_or("30".to_string())
        .parse()?;

    let retry_threshold: u32 = match env::var("RETRY_THRESHOLD") {
        Ok(val) => val.parse().unwrap_or(3),
        Err(_) => 3,
    };

    let ping_count: u32 = match env::var("PING_COUNT") {
        Ok(val) => val.parse().unwrap_or(5),
        Err(_) => 5,
    };

    let (tx, rx) = broadcast::channel(1);

    let kube_client = Client::try_default().await?;
    let ewma_latency_by_host = Arc::new(Mutex::new(HashMap::new()));
    let ewma_cpu_by_host = Arc::new(Mutex::new(HashMap::new()));
    let nftables_chain_by_service = Arc::new(Mutex::new(HashMap::new()));

    let mut latency_prober = LatencyProber {
        proc_sleep: Duration::from_secs(delay),
        shutdown_sig: rx,
        retry_threshold,
        ping_count,
        kube_client: kube_client.clone(),
        ewma_latency_by_host: ewma_latency_by_host.clone(),
    };
    let mut cpu_watcher = CpuCollector {
        proc_sleep: Duration::from_secs(delay),
        shutdown_sig: tx.subscribe(),
        retry_threshold,
        kube_client: kube_client.clone(),
        ewma_cpu_by_host: ewma_cpu_by_host.clone(),
    };
    let mut nftables_watcher = NftablesWatcher {
        shutdown_sig: tx.subscribe(),
        kube_client: kube_client.clone(),
        nftables_chain_by_service: nftables_chain_by_service.clone(),
    };
    let mut nftables_balancer = NftablesBalancer {
        proc_sleep: Duration::from_secs(delay),
        shutdown_sig: tx.subscribe(),
        retry_threshold,
        nftables_chain_by_service: nftables_chain_by_service.clone(),
        ewma_latency_by_host: ewma_latency_by_host.clone(),
        ewma_cpu_by_host: ewma_cpu_by_host.clone(),
    };

    tokio::spawn(async move { latency_prober.run().await });
    tokio::spawn(async move { cpu_watcher.run().await });
    tokio::spawn(async move { nftables_watcher.run().await });
    tokio::spawn(async move { nftables_balancer.run().await });

    let router = Router::new().route(
        "/scores",
        routing::get({
            let ewma_latency_lookup = ewma_latency_by_host.clone();
            async move || {
                let mut res = vec![];
                let ewma_latency_lookup = ewma_latency_lookup.lock().await;
                for (k, v) in ewma_latency_lookup.iter() {
                    res.push(ProbeResult {
                        host: k.clone(),
                        score: *v,
                    });
                }
                axum::Json(res)
            }
        }),
    );

    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown(tx))
        .await?;

    Ok(())
}

async fn shutdown(tx: Sender<()>) {
    let sigint = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen SIGINT");
    };

    #[cfg(unix)]
    let sigterm = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen SIGTERM")
            .recv()
            .await;
    };

    tokio::select! {
        _ = sigint => { tx.send(()).expect("failed to send shutdown signal"); },
        _ = sigterm => { tx.send(()).expect("failed to send shutdown signal"); },
    }
}
