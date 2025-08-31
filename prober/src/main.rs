use std::{collections::HashMap, env, sync::Arc, time::Duration};

use anyhow::Result;
use axum::{Router, routing};
use futures::lock::Mutex;
use kube::Client;
use prober::probe::{ProbeResult, Prober};
use tokio::{net::TcpListener, time};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let delay = env::var("INTERVAL_IN_SECONDS")?.parse()?;

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

    let cost_caches_probe = cost_caches.clone();
    tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_secs(delay)).await;
            let mut cost_caches = cost_caches_probe.lock().await;
            let mut prober = Prober::new(&mut cost_caches, client.clone());
            prober.probe(retry_threshold, ping_count).await;
        }
    });

    let cost_caches_http = cost_caches.clone();
    let router = Router::new().route(
        "/scores",
        routing::get(async move || {
            let cost_caches = cost_caches_http.clone();
            let cost_caches = cost_caches.lock().await;
            let mut res = vec![];
            for (k, v) in cost_caches.iter() {
                res.push(ProbeResult {
                    host: k.clone(),
                    score: v.to_bits(),
                });
            }
            axum::Json(res)
        }),
    );
    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, router).await?;

    Ok(())
}
