use anyhow::Result;
use prober::{cilium, probe};
use std::{collections::HashMap, env, time::Duration};
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let delay = env::var("INTERVAL_IN_SECONDS")?.parse()?;
    let mut cost_caches = HashMap::new();

    let retry_threshold: u32 = match env::var("RETRY_THRESHOLD") {
        Ok(val) => val.parse().unwrap_or(3),
        Err(_) => 3,
    };

    let ping_count: u32 = match env::var("PING_COUNT") {
        Ok(val) => val.parse().unwrap_or(5),
        Err(_) => 5,
    };

    let node_name = env::var("NODE_NAME")?;
    let service_name = env::var("CILIUM_SERVICE_NAME")?;

    loop {
        probe::probe(retry_threshold, ping_count, &mut cost_caches).await;

        if let Err(e) =
            cilium::update_cilium_service_weights(&node_name, &service_name, &cost_caches).await
        {
            tracing::error!("Failed to update cilium service weights: {}", e);
        }

        time::sleep(Duration::from_secs(delay)).await;
    }
}
