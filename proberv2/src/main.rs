use std::{collections::HashMap, env, path::Path};

use proberv2::{actor::Actor, config::Config};
use tokio::{
    fs,
    signal::unix::{self, SignalKind},
};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("prober: program starting");

    let config_path = env::var("CONFIG_PATH")?;
    let config_path = Path::new(&config_path);
    let config = fs::read_to_string(config_path).await?;
    let mut config: Config = serde_json::from_str(&config)?;

    let node_name = env::var("NODENAME")?;
    config.node_name = node_name;

    let (tx, rx) = tokio::sync::broadcast::channel(32);
    {
        let mut actor = Actor {
            config,
            datapoint_by_nodename: HashMap::new(),
            service_by_nodeport: HashMap::new(),
        };
        actor.setup_nftables().await?;

        tokio::spawn(async move { actor.dispatch(tx).await });

        let mut sigint = unix::signal(SignalKind::interrupt())?;
        let mut sigterm = unix::signal(SignalKind::terminate())?;

        tokio::select! {
            _ = sigint.recv() => {},
            _ = sigterm.recv() => {},
        }
    }

    Ok(())
}
