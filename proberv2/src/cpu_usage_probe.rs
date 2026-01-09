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
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub async fn probe_cpu_usage(
    config: Config,
    tx: broadcast::Sender<Event>,
    token: CancellationToken,
) -> anyhow::Result<()> {
    let mut ticker = interval(Duration::from_secs(config.probe.cpu_interval));
    let mut nodes = HashSet::<WorkerNode>::new();
    let mut datapoint_by_nodename = HashMap::<String, f64>::new();

    let mut rx = tx.subscribe();
    'main: loop {
        // i hope this works
        if token.is_cancelled() {
            info!("actor: exiting probe_cpu_usage task");
            return Ok(());
        }

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
            let cpu_usage = data.sample().value();

            let datapoint = match datapoint_by_nodename.get(&worker.name) {
                Some(datapoint) => {
                    config.alpha.ewma_cpu * cpu_usage + (1.0 - config.alpha.ewma_cpu) * *datapoint
                }
                None => cpu_usage,
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
