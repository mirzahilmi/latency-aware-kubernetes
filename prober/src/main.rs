use kube::Client;
use prober::{
    cpu_watcher::CpuCollector, latency_prober::LatencyProber,
    nftables_reconciler::NftablesReconciler, nftables_watcher::NftablesWatcher,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, sync::Arc, time::Duration};

use axum::{Router, routing};
use futures::lock::Mutex;
use tokio::{
    net::TcpListener,
    sync::broadcast::{self, Sender},
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeResult {
    hostname: String,
    cpu_ewma_score: f64,
    latency_ewma_score: f64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let delay = env::var("INTERVAL_IN_SECONDS")?;
    let delay: u64 = delay.parse()?;

    let retry_threshold = env::var("RETRY_THRESHOLD")?;
    let retry_threshold: u32 = retry_threshold.parse()?;

    let ping_count = env::var("PING_COUNT")?;
    let ping_count: u32 = ping_count.parse()?;

    let service_level_agreement = env::var("SERVICE_LEVEL_AGREEMENT")?;
    let service_level_agreement: u32 = service_level_agreement.parse()?;

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
        service_level_agreement,
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
    let mut nftables_reconciler = NftablesReconciler {
        shutdown_sig: tx.subscribe(),
        retry_threshold,
        nftables_chain_by_service: nftables_chain_by_service.clone(),
        ewma_latency_by_host: ewma_latency_by_host.clone(),
        ewma_cpu_by_host: ewma_cpu_by_host.clone(),
    };

    tokio::spawn(async move { latency_prober.run().await });
    tokio::spawn(async move { cpu_watcher.run().await });
    tokio::spawn(async move { nftables_watcher.run().await });
    tokio::spawn(async move { nftables_reconciler.run().await });

    let router = Router::new().route(
        "/scores",
        routing::get({
            let ewma_latency_by_host = ewma_latency_by_host.clone();
            let ewma_cpu_by_host = ewma_cpu_by_host.clone();
            async move || {
                let mut res = vec![];
                let ewma_latency_by_host = ewma_latency_by_host.lock().await;
                let ewma_cpu_by_host = ewma_cpu_by_host.lock().await;
                ewma_latency_by_host
                    .iter()
                    .for_each(|(host, ewma_latency)| {
                        let Some(ewma_cpu) = ewma_cpu_by_host.get(host) else {
                            return;
                        };
                        res.push(ProbeResult {
                            hostname: host.clone(),
                            cpu_ewma_score: *ewma_cpu,
                            latency_ewma_score: *ewma_latency,
                        });
                    });
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
