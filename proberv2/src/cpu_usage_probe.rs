use std::{
    collections::{HashMap, HashSet},
    env,
    net::IpAddr,
};

use crate::actor::{Actor, Event, EwmaDatapoint, WorkerNode};
use prometheus_http_query::Client;
use tokio::{
    sync::broadcast,
    time::{Duration, interval},
};
use tracing::{debug, error, info, warn};

impl Actor {
    pub async fn probe_cpu_usage(
        tx: broadcast::Sender<Event>,
        mut rx: broadcast::Receiver<Event>,
    ) -> anyhow::Result<()> {
        // should be managed by global config
        let prometheus_url = env::var("PROMETHEUS_URL")?;

        let mut ticker = interval(Duration::from_secs(15));
        let mut nodes = HashSet::<WorkerNode>::new();
        let mut datapoint_by_nodename = HashMap::<IpAddr, f64>::new();

        'main: loop {
            ticker.tick().await;

            while let Ok(event) = rx.try_recv() {
                // should handle node removal
                if let Event::NodeJoined(node) = event {
                    nodes.insert(node);
                }
            }

            // should be better off query once then iterate on the result
            for worker in &nodes {
                let client = match Client::try_from(prometheus_url.clone()) {
                    Ok(client) => client,
                    Err(e) => {
                        error!("actor: failed to create prometheus client: {e}");
                        continue;
                    }
                };

                let query = format!(
                    // thanks to https://stackoverflow.com/a/66263640
                    r#"avg(1 - rate(node_cpu_seconds_total{{mode="idle",instance="{}:9100"}}[5m])) by (instance)"#,
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
                let usage = data.sample().value(); // already normalized
                let alpha = 0.4;
                let datapoint = match datapoint_by_nodename.get(&worker.ip) {
                    Some(datapoint) => alpha * usage + (1.0 - alpha) * *datapoint,
                    None => usage,
                };
                debug!(
                    "actor: datapoint calculation result for {}:{} is {}",
                    worker.name, worker.ip, datapoint,
                );
                datapoint_by_nodename.insert(worker.ip, datapoint);
                if let Err(e) = tx.send(Event::EwmaCalculated(
                    worker.clone(),
                    EwmaDatapoint::Cpu(datapoint),
                )) {
                    info!("actor: latency probe exiting: {e}");
                    break 'main;
                };
            }
        }

        Ok(())
    }
}
