use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use crate::config::Config;

use super::actor::{Event, EwmaDatapoint, WorkerNode};
use tokio::{
    sync::broadcast,
    time::{Duration, Instant, interval},
};
use tracing::{debug, error, info};

pub async fn probe_latency(config: Config, tx: broadcast::Sender<Event>) -> anyhow::Result<()> {
    let mut ticker = interval(Duration::from_secs(15));
    let mut nodes = HashSet::<WorkerNode>::new();
    let mut datapoint_by_nodename = HashMap::<IpAddr, f64>::new();

    let mut rx = tx.subscribe();
    'main: loop {
        while let Ok(event) = rx.try_recv() {
            if let Event::NodeJoined(node) = event {
                nodes.insert(node);
            }
        }

        for worker in &nodes {
            let now = Instant::now();
            if let Err(e) = reqwest::get(format!("http://{}:{}", worker.ip, config.app_port)).await
            {
                error!("actor: failed to probe http request: {e}");
                continue;
            };

            // scary
            let normalized_data =
                (now.elapsed().as_millis() / config.service_level_agreement) as f64;
            debug!(
                "actor: latency probe of {} takes {} ms",
                worker.ip,
                now.elapsed().as_millis()
            );

            let alpha = if normalized_data > 1.0 {
                1.0
            } else if normalized_data > 0.6 {
                0.8
            } else if normalized_data > 0.4 {
                0.6
            } else if normalized_data > 0.2 {
                0.4
            } else {
                0.2
            };

            let datapoint = match datapoint_by_nodename.get(&worker.ip) {
                Some(datapoint) => alpha * normalized_data + (1.0 - alpha) * *datapoint,
                None => normalized_data,
            };
            datapoint_by_nodename.insert(worker.ip, datapoint);
            if let Err(e) = tx.send(Event::EwmaCalculated(
                worker.clone(),
                EwmaDatapoint::Latency(datapoint),
            )) {
                info!("actor: latency probe exiting: {e}");
                break 'main;
            };
        }

        ticker.tick().await;
    }

    Ok(())
}
