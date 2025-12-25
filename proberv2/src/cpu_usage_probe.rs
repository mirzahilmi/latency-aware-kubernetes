use std::collections::{HashMap, HashSet};

use crate::{
    actor::{Event, EwmaDatapoint, WorkerNode},
    config::Config,
};
use prometheus_http_query::Client;
use tokio::{
    sync::broadcast,
    time::{Duration, interval},
};
use tracing::{error, info, warn};

pub async fn probe_cpu_usage(config: Config, tx: broadcast::Sender<Event>) -> anyhow::Result<()> {
    let mut ticker = interval(Duration::from_secs(config.probe.cpu_interval));
    let mut nodes = HashSet::<WorkerNode>::new();
    let mut datapoint_by_nodename = HashMap::<String, f64>::new();

    let mut rx = tx.subscribe();
    'main: loop {
        while let Ok(event) = rx.try_recv() {
            // should handle node removal
            if let Event::NodeJoined(node) = event {
                nodes.insert(node);
            }
        }

        // should be better off query once then iterate on the result
        for worker in &nodes {
            let client = match Client::try_from(config.prometheus.url.clone()) {
                Ok(client) => client,
                Err(e) => {
                    error!("actor: failed to create prometheus client: {e}");
                    continue;
                }
            };

            let query = format!(
                // thanks to https://stackoverflow.com/a/66263640
                r#"(1 - avg(irate(node_cpu_seconds_total{{mode="idle",instance="{}:9100"}}[5m])) by (instance))"#,
                worker.ip,
            );

            let response = match client.query(query).get().await {
                Ok(response) => response,
                Err(e) => {
                    error!(
                        "actor: failed to query node {} cpu usage: {}",
                        worker.name, e
                    );
                    continue;
                }
            };
            let datas = match response.data().as_vector() {
                Some(datas) => datas,
                None => {
                    warn!("actor: promql result is not a vector: {:?}", response);
                    continue;
                }
            };
            let Some(data) = datas.first() else {
                warn!("actor: empty promql result");
                continue;
            };
            let normalized_data = data.sample().value();

            let alpha = if normalized_data > 0.8 {
                0.8
            } else if normalized_data > 0.5 {
                0.6
            } else if normalized_data > 0.2 {
                0.4
            } else {
                0.2
            };

            let datapoint = match datapoint_by_nodename.get(&worker.name) {
                Some(datapoint) => alpha * normalized_data + (1.0 - alpha) * *datapoint,
                None => normalized_data,
            };
            datapoint_by_nodename.insert(worker.name.clone(), datapoint);
            if let Err(e) = tx.send(Event::EwmaCalculated(
                worker.name.clone(),
                EwmaDatapoint::Cpu(datapoint),
            )) {
                info!("actor: latency probe exiting: {e}");
                break 'main;
            };
        }

        ticker.tick().await;
    }

    Ok(())
}
