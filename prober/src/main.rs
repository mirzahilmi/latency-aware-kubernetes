use std::{collections::HashMap, env, sync::Arc, time::Duration};

use anyhow::Result;
use axum::{Router, routing};
use futures::lock::Mutex;
use kube::Client;
use prober::{
    balancer::{self},
    probe::{ProbeResult, Prober},
};
use tokio::{net::TcpListener, time};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let delay: u64 = env::var("INTERVAL_IN_SECONDS")?.parse()?;

    let retry_threshold: u32 = match env::var("RETRY_THRESHOLD") {
        Ok(val) => val.parse().unwrap_or(3),
        Err(_) => 3,
    };

    let ping_count: u32 = match env::var("PING_COUNT") {
        Ok(val) => val.parse().unwrap_or(5),
        Err(_) => 5,
    };

    let client = Client::try_default().await?;
    let cost_caches = Arc::new(Mutex::new(HashMap::new()));

    let mut prober = Prober::new(
        client.clone(),
        cost_caches.clone(),
        Duration::from_secs(delay),
    );
    tokio::spawn(async move { prober.watch_probe(retry_threshold, ping_count).await });

    let nftables_chains_lookup = Arc::new(Mutex::new(HashMap::new()));
    tokio::spawn(balancer::watch_service_updates(
        client.clone(),
        nftables_chains_lookup.clone(),
    ));
    tokio::spawn({
        let cost_caches = cost_caches.clone();
        let nftables_chains_lookup = nftables_chains_lookup.clone();
        async move {
            loop {
                time::sleep(Duration::from_secs(delay)).await;
                let _ =
                    balancer::reconcile(cost_caches.clone(), nftables_chains_lookup.clone()).await;
            }
        }
    });

    let router = Router::new().route(
        "/scores",
        routing::get({
            let cost_caches = cost_caches.clone();
            async move || {
                let mut res = vec![];
                let cost_caches = cost_caches.lock().await;
                for (k, v) in cost_caches.iter() {
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
    axum::serve(listener, router).await?;

    Ok(())
}
