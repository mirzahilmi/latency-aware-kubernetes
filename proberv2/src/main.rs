use std::{collections::HashMap, env, path::Path, time::Duration};

use proberv2::{actor::Actor, config::Config};
use tokio::{
    fs,
    signal::unix::{self, SignalKind},
};
use tokio_util::sync::CancellationToken;
use tracing::info;

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

    let token = CancellationToken::new();
    let child_token = token.clone();

    let mut actor = Actor {
        config: config.clone(),
        datapoint_by_nodename: HashMap::new(),
        service_by_nodeport: HashMap::new(),
    };
    actor.setup_nftables().await?;

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
