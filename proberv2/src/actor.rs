use std::{collections::HashMap, net::IpAddr};
use tokio::sync::broadcast::{Receiver, Sender};
use tracing::{info, warn};

pub struct Actor {
    pub tx: Sender<Event>,
    pub rx: Receiver<Event>,
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
    pub async fn dispatch(&mut self) {
        tokio::spawn(Actor::watch_nodes(self.tx.clone()));
        tokio::spawn(Actor::probe_latency(self.tx.clone(), self.tx.subscribe()));
        tokio::spawn(Actor::probe_cpu_usage(self.tx.clone(), self.tx.subscribe()));

        while let Ok(event) = self.rx.recv().await {
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
