use std::{
    collections::{HashMap, HashSet},
    env,
    net::IpAddr,
};

use crate::actor::{Actor, Event, EwmaDatapoint, WorkerNode};
use tokio::{
    sync::broadcast,
    time::{Duration, interval},
};
use tracing::{debug, error, info};

impl Actor {
    pub async fn probe_latency(
        tx: broadcast::Sender<Event>,
        mut rx: broadcast::Receiver<Event>,
    ) -> anyhow::Result<()> {
        // should be managed by global config
        let sla = env::var("SERVICE_LEVEL_AGREEMENT")?;
        let sla: u64 = sla.parse()?;

        let mut ticker = interval(Duration::from_secs(15));
        let mut nodes = HashSet::<WorkerNode>::new();
        let mut datapoint_by_nodename = HashMap::<IpAddr, f64>::new();

        'main: loop {
            ticker.tick().await;

            while let Ok(event) = rx.try_recv() {
                if let Event::NodeJoined(node) = event {
                    nodes.insert(node);
                }
            }

            for worker in &nodes {
                let result = match ping::new(worker.ip).socket_type(ping::DGRAM).send() {
                    Ok(result) => result,
                    Err(e) => {
                        error!(
                            "actor: failed to ping node {}:{}: {}",
                            worker.name,
                            worker.ip.to_string(),
                            e,
                        );
                        continue;
                    }
                };
                let normalized = (result.rtt.as_secs() / sla) as f64;
                let alpha = 0.7;
                let datapoint = match datapoint_by_nodename.get(&worker.ip) {
                    Some(datapoint) => alpha * normalized + (1.0 - alpha) * *datapoint,
                    None => normalized,
                };
                debug!(
                    "actor: datapoint calculation result for {}:{} is {}",
                    worker.name,
                    worker.ip.to_string(),
                    datapoint,
                );
                datapoint_by_nodename.insert(worker.ip, datapoint);
                if let Err(e) = tx.send(Event::EwmaCalculated(
                    worker.clone(),
                    EwmaDatapoint::Latency(datapoint),
                )) {
                    info!("actor: latency probe exiting: {e}");
                    break 'main;
                };
            }
        }

        Ok(())
    }
}
