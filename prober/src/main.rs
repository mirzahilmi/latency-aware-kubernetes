use anyhow::Result;
use kube::Client;
use prober::probe::Prober;
use std::{collections::HashMap, env, time::Duration};
use tokio::time;

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
    let mut cost_caches = HashMap::new();
    let mut prober = Prober::new(&mut cost_caches, client.clone());

    loop {
        prober.probe(retry_threshold, ping_count).await;
        time::sleep(Duration::from_secs(delay)).await;
    }
}

