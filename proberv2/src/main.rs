use std::collections::HashMap;

use proberv2::actor::Actor;
use tokio::signal::unix::{self, SignalKind};
use tracing::error;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let (tx, rx) = tokio::sync::broadcast::channel(32);
    {
        let mut actor = Actor {
            datapoint_by_nodename: HashMap::new(),
        };

        tokio::spawn(async move { actor.dispatch(tx).await });

        let mut sigint = match unix::signal(SignalKind::interrupt()) {
            Ok(sig) => sig,
            Err(e) => {
                error!("main: failed to listen for SIGINT: {e}");
                return;
            }
        };
        let mut sigterm = match unix::signal(SignalKind::terminate()) {
            Ok(sig) => sig,
            Err(e) => {
                error!("main: failed to listen for SIGTERM: {e}");
                return;
            }
        };

        tokio::select! {
            _ = sigint.recv() => return,
            _ = sigterm.recv() => return,
        }
    }
}
