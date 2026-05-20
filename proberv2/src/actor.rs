use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    time,
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::{
    config::Config, cpu_usage_probe::probe_cpu_usage, endpoints_watch::watch_endpoints,
    latency_probe::probe_latency, node_watch::watch_nodes, update_nftables::update_nftables,
};

pub struct Actor {
    pub config: Config,
    pub datapoint_by_nodename: HashMap<String, Option<ScorePair>>,
    pub service_by_nodeport: HashMap<i32, Service>,
}

#[derive(Clone)]
pub enum Event {
    ServiceChanged(Service),
    // String == Node Name, probably should separate type?
    EwmaCalculated(String, EwmaDatapoint),
    NodeJoined(WorkerNode),
}

#[derive(Eq, Hash, PartialEq, Clone, Debug)]
pub struct WorkerNode {
    pub name: String,
    pub ip: IpAddr,
}

#[derive(Debug, Default, Clone)]
pub struct ScorePair {
    pub latency: f64,
    pub cpu: f64,
}

#[derive(Clone)]
pub enum EwmaDatapoint {
    Latency(f64),
    Cpu(f64),
}

#[derive(Clone, Debug)]
pub struct Service {
    pub name: String,
    pub nodeport: i32,
    pub targetport: i32,
    pub endpoints_by_nodename: HashMap<String, Vec<Ipv4Addr>>,
}

impl Actor {
    pub async fn dispatch(&mut self, token: CancellationToken) {
        info!("actor: starting processes");
        let (tx, mut rx) = broadcast::channel(32);

        tokio::spawn({
            let token = token.clone();
            watch_nodes(tx.clone(), token)
        });
        tokio::spawn({
            let token = token.clone();
            probe_latency(self.config.clone(), tx.clone(), token)
        });
        tokio::spawn({
            let token = token.clone();
            probe_cpu_usage(self.config.clone(), tx.clone(), token)
        });
        tokio::spawn({
            let token = token.clone();
            watch_endpoints(self.config.clone(), tx.clone(), token)
        });

        let mut ticker = time::interval(Duration::from_secs(self.config.probe.nft_update_interval));
        'main: loop {
            let event = tokio::select! {
                event = rx.recv() => event,
                _ = token.cancelled() => {
                    info!("actor: exiting main actor dispatch");
                    break 'main
                },
                _ = ticker.tick() => {
                    for service in self.service_by_nodeport.values() {
                        if let Err(e) = update_nftables(
                            self.config.clone(),
                            service.clone(),
                            self.datapoint_by_nodename.clone(),
                        )
                        .await
                        {
                            error!("actor: reacting to service endpoints update failed: {e}");
                        };
                    }
                    continue 'main
                }
            };
            let event = match event {
                Ok(event) => event,
                Err(RecvError::Closed) => break 'main,
                _ => continue,
            };

            match event {
                Event::ServiceChanged(service) => {
                    self.service_by_nodeport
                        .insert(service.nodeport, service.clone());
                    if let Err(e) = update_nftables(
                        self.config.clone(),
                        service,
                        self.datapoint_by_nodename.clone(),
                    )
                    .await
                    {
                        error!("actor: reacting to service endpoints update failed: {e}");
                    };
                }
                Event::EwmaCalculated(worker, dp) => {
                    let Some(score) = self.datapoint_by_nodename.get_mut(&worker) else {
                        warn!("actor: ghost node {} got ewma calculation", worker);
                        continue;
                    };
                    let score = score.get_or_insert_with(ScorePair::default);

                    match dp {
                        EwmaDatapoint::Latency(v) => score.latency = v,
                        EwmaDatapoint::Cpu(v) => score.cpu = v,
                    }

                    info!(
                        "actor: updated node {} with latency {} cpu {}",
                        worker, score.latency, score.cpu
                    );
                }
                Event::NodeJoined(worker) => {
                    self.datapoint_by_nodename
                        .entry(worker.name)
                        .or_insert(None);
                }
            }
        }
    }
}
