use std::{collections::HashMap, net::IpAddr};
use tokio::sync::broadcast::Sender;
use tracing::{info, warn};

use crate::{
    cpu_usage_probe::probe_cpu_usage, endpoints_watch::watch_endpoints,
    latency_probe::probe_latency, node_watch::watch_nodes,
};

pub struct Actor {
    pub datapoint_by_nodename: HashMap<WorkerNode, Option<ScorePair>>,
}

#[derive(Clone)]
pub enum Event {
    ServiceChanged(Service),
    EwmaCalculated(WorkerNode, EwmaDatapoint),
    NodeJoined(WorkerNode),
}

#[derive(Eq, Hash, PartialEq, Clone, Debug)]
pub struct WorkerNode {
    pub name: String,
    pub ip: IpAddr,
}

#[derive(Debug, Default)]
pub struct ScorePair {
    pub latency: f64,
    pub cpu: f64,
}

#[derive(Clone)]
pub enum EwmaDatapoint {
    Latency(f64),
    Cpu(f64),
}

#[derive(Clone)]
pub struct Service {
    pub name: String,
    pub ip: IpAddr,
    pub endpoints_by_nodename: HashMap<String, Vec<String>>,
}

impl Actor {
    pub async fn dispatch(&mut self, tx: Sender<Event>) {
        tokio::spawn(watch_nodes(tx.clone()));
        tokio::spawn(probe_latency(tx.clone()));
        tokio::spawn(probe_cpu_usage(tx.clone()));
        tokio::spawn(watch_endpoints(tx.clone()));

        let mut rx = tx.subscribe();
        while let Ok(event) = rx.recv().await {
            match event {
                Event::ServiceChanged(_) => {
                    // not implemented yet
                }
                Event::EwmaCalculated(worker, dp) => {
                    let Some(slot) = self.datapoint_by_nodename.get_mut(&worker) else {
                        warn!(
                            "actor: ghost node {}:{} got ewma calculation",
                            worker.name, worker.ip
                        );
                        continue;
                    };
                    let slot = slot.get_or_insert_with(ScorePair::default);

                    match dp {
                        EwmaDatapoint::Latency(v) => slot.latency = v,
                        EwmaDatapoint::Cpu(v) => slot.cpu = v,
                    }

                    info!(
                        "actor: updated node {}:{} with latency {} cpu {}",
                        worker.name, worker.ip, slot.latency, slot.cpu
                    );
                }
                Event::NodeJoined(worker) => {
                    self.datapoint_by_nodename.insert(worker, None);
                }
            }
        }
    }
}
