use anyhow::anyhow;
use k8s_openapi::api::core::v1::Node;
use kube::{Api, Client, ResourceExt};
use nftables::{
    batch::Batch,
    expr::Expression as NftExpression,
    helper,
    schema::{Chain, NfListObject, Rule, Set, SetType, SetTypeValue, Table},
    stmt::{JumpTarget, Statement},
    types::{NfChainPolicy, NfChainType, NfFamily, NfHook},
};
use serde_json::json;
use std::{
    borrow::Cow,
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
};
use tokio::sync::broadcast::{Sender, error::RecvError};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

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
    // String == Node Name, probably should separate separate type?
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
    pub async fn dispatch(&mut self, tx: Sender<Event>, token: CancellationToken) {
        info!("actor: starting processes");

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

        let mut rx = tx.subscribe();
        'main: loop {
            let event = tokio::select! {
                _ = token.cancelled() => {
                    info!("actor: exiting main actor dispatch");
                    break 'main
                }
                event = rx.recv() => event,
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
                    let Some(slot) = self.datapoint_by_nodename.get_mut(&worker) else {
                        warn!("actor: ghost node {} got ewma calculation", worker);
                        continue;
                    };
                    let slot = slot.get_or_insert_with(ScorePair::default);

                    match dp {
                        EwmaDatapoint::Latency(v) => slot.latency = v,
                        EwmaDatapoint::Cpu(v) => slot.cpu = v,
                    }

                    info!(
                        "actor: updated node {} with latency {} cpu {}",
                        worker, slot.latency, slot.cpu
                    );

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
                }
                Event::NodeJoined(worker) => {
                    self.datapoint_by_nodename
                        .entry(worker.name)
                        .or_insert(None);
                }
            }
        }
    }

    pub async fn setup_nftables(&self) -> anyhow::Result<()> {
        info!("actor: configuring base nftables ruleset");
        let client = Client::try_default().await?;
        let api: Api<Node> = Api::all(client);
        let node = api.get(&self.config.kubernetes.node_name).await?;
        let Some(addrs) = node
            .status
            .as_ref()
            .and_then(|status| status.addresses.as_ref())
        else {
            return Err(anyhow!(
                "missing node {} addresses attribute",
                node.name_any(),
            ));
        };
        let Some(a) = addrs.iter().find(|x| x.type_ == "InternalIP") else {
            return Err(anyhow!(
                "missing node {} InternalIP address",
                node.name_any(),
            ));
        };

        let ip = a.address.parse::<IpAddr>()?;

        let mut batch = Batch::new();
        batch.delete(NfListObject::Table(Table {
            name: self.config.nftables.table.clone().into(),
            family: NfFamily::IP,
            ..Default::default()
        }));
        let ruleset = batch.to_nftables();
        let _ = helper::apply_ruleset(&ruleset); // ignoring deletion error

        let mut batch = Batch::new();
        batch.add(NfListObject::Table(Table {
            name: self.config.nftables.table.clone().into(),
            family: NfFamily::IP,
            ..Default::default()
        }));

        let ruleset = batch.to_nftables();
        debug!(
            "actor: applying table: {}",
            serde_json::to_string(&ruleset)?
        );
        helper::apply_ruleset(&ruleset)?;
        let mut batch = Batch::new();

        // used raw because the crate does not has map type of `verdict`
        helper::apply_ruleset_raw(
            json!({
              "nftables": [
                {
                  "add": {
                    "map": {
                      "family": "ip",
                      "table": self.config.nftables.table,
                      "name": self.config.nftables.map_service_chain_by_nodeport,
                      "type": [
                        "inet_proto",
                        "inet_service"
                      ],
                      "map": "verdict",
                      "comment": "VERDICTS! MUAHAHAHAHA"
                    }
                  }
                }
              ]
            })
            .to_string()
            .as_ref(),
            None::<&str>,
            std::iter::empty::<&str>(),
        )?;

        let ip_sets = [NftExpression::String(ip.to_string().into())];
        batch.add(NfListObject::Set(
            Set {
                family: NfFamily::IP,
                table: self.config.nftables.table.clone().into(),
                name: self.config.nftables.set_allowed_node_ips.clone().into(),
                set_type: SetTypeValue::Single(SetType::Ipv4Addr),
                comment: Some("List IPv4 yang nerima traffic dari NodePort".into()),
                elem: Some(ip_sets.as_ref().into()),
                ..Default::default()
            }
            .into(),
        ));

        let ruleset = batch.to_nftables();
        debug!(
            "actor: applying set ruleset: {}",
            serde_json::to_string(&ruleset)?
        );
        helper::apply_ruleset(&ruleset)?;
        let mut batch = Batch::new();

        batch.add(NfListObject::Chain(Chain {
            family: NfFamily::IP,
            table: self.config.nftables.table.clone().into(),
            name: self.config.nftables.chain_prerouting.clone().into(),
            _type: NfChainType::NAT.into(),
            hook: NfHook::Prerouting.into(),
            prio: (-150).into(),
            policy: NfChainPolicy::Accept.into(),
            ..Default::default()
        }));
        batch.add(NfListObject::Chain(Chain {
            family: NfFamily::IP,
            table: self.config.nftables.table.clone().into(),
            name: self.config.nftables.chain_services.clone().into(),
            ..Default::default()
        }));

        let ruleset = batch.to_nftables();
        debug!(
            "actor: applying chain: {}",
            serde_json::to_string(&ruleset)?
        );
        helper::apply_ruleset(&ruleset)?;
        let mut batch = Batch::new();

        batch.add(NfListObject::Rule(Rule {
            family: NfFamily::IP,
            table: self.config.nftables.table.clone().into(),
            chain: self.config.nftables.chain_prerouting.clone().into(),
            expr: Cow::Owned(vec![Statement::Jump(JumpTarget {
                target: self.config.nftables.chain_services.clone().into(),
            })]),
            ..Default::default()
        }));
        let ruleset = batch.to_nftables();
        debug!(
            "actor: applying chain: {}",
            serde_json::to_string(&ruleset)?
        );
        helper::apply_ruleset(&ruleset)?;

        let rule = json!(
        {
          "nftables": [
            {
              "add": {
                "rule": {
                  "family": "ip",
                  "table": self.config.nftables.table,
                  "chain": self.config.nftables.chain_services,
                  "comment": "Cek IPv4 paket di list IPv4 NodePort, kalo ada langsung ke verdict map ke service yang sesuai",
                  "expr": [
                    {
                      "match": {
                        "op": "==",
                        "left": {
                          "payload": {
                            "protocol": "ip",
                            "field": "daddr"
                          }
                        },
                        "right": format!("@{}", self.config.nftables.set_allowed_node_ips)
                      }
                    },
                    {
                      "vmap": {
                        "key": {
                          "concat": [
                            {
                              "meta": {
                                "key": "l4proto"
                              }
                            },
                            {
                              "payload": {
                                "protocol": "th",
                                "field": "dport"
                              }
                            }
                          ]
                        },
                        "data": format!("@{}", self.config.nftables.map_service_chain_by_nodeport)
                      }
                    }
                  ]
                }
              }
            }
          ]
        }
        );

        debug!("actor: applying initial ruleset: {}", rule.to_string());
        helper::apply_ruleset_raw(&rule.to_string(), None::<&str>, std::iter::empty::<&str>())?;

        Ok(())
    }
}
