use anyhow::anyhow;
use k8s_openapi::api::core::v1::Node;
use kube::{Api, Client, ResourceExt};
use nftables::{
    batch::Batch,
    expr::{Expression as NftExpression, Meta, MetaKey, NamedExpression, Payload, PayloadField},
    helper,
    schema::{Chain, NfListObject, Rule, Set, SetType, SetTypeValue, Table},
    stmt::{JumpTarget, Match, Operator, Statement, VerdictMap},
    types::{NfChainPolicy, NfChainType, NfFamily, NfHook},
};
use serde_json::json;
use std::{collections::HashMap, net::IpAddr};
use tokio::sync::broadcast::Sender;
use tracing::{debug, info, warn};

use crate::{
    config::Config, cpu_usage_probe::probe_cpu_usage, endpoints_watch::watch_endpoints,
    latency_probe::probe_latency, node_watch::watch_nodes,
};

pub struct Actor {
    pub datapoint_by_nodename: HashMap<WorkerNode, Option<ScorePair>>,
    pub config: Config,
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
        info!("actor: starting processes");

        tokio::spawn(watch_nodes(tx.clone()));
        // probably shouldn't use clone here, at the time idk any better
        tokio::spawn(probe_latency(self.config.clone(), tx.clone()));
        tokio::spawn(probe_cpu_usage(self.config.clone(), tx.clone()));
        tokio::spawn(watch_endpoints(self.config.clone(), tx.clone()));

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

    pub async fn setup_nftables(&self) -> anyhow::Result<()> {
        info!("actor: configuring base nftables ruleset");
        let client = Client::try_default().await?;
        let api: Api<Node> = Api::all(client);
        let node = api.get(&self.config.node_name).await?;
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

        batch.add(NfListObject::Table(Table {
            name: self.config.nftables.table.clone().into(),
            family: NfFamily::IP,
            ..Default::default()
        }));

        let ruleset = batch.to_nftables();
        debug!("actor: applying table: {ruleset:?}");
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
                      "comment": "VERDICTS! MUAHAHAHAHA",
                      "map": "verdict",
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
        debug!("actor: applying set ruleset: {ruleset:?}");
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
        debug!("actor: applying chain: {ruleset:?}");
        helper::apply_ruleset(&ruleset)?;
        let mut batch = Batch::new();

        let jump_to_services = [Statement::Jump(JumpTarget {
            target: self.config.nftables.chain_services.clone().into(),
        })];
        batch.add(NfListObject::Rule(Rule {
            family: NfFamily::IP,
            table: self.config.nftables.table.clone().into(),
            chain: self.config.nftables.chain_prerouting.clone().into(),
            expr: jump_to_services.as_ref().into(),
            handle: Some(0),
            ..Default::default()
        }));

        let vmap_to_service_lookup = [
            Statement::Match(Match {
                left: NftExpression::Named(NamedExpression::Payload(Payload::PayloadField(
                    PayloadField {
                        protocol: "ip".into(),
                        field: "daddr".into(),
                    },
                ))),
                op: Operator::EQ,
                right: NftExpression::String(
                    format!("@{}", self.config.nftables.set_allowed_node_ips.clone()).into(),
                ),
            }),
            Statement::VerdictMap(VerdictMap {
                key: NftExpression::Named(NamedExpression::Concat(vec![
                    NftExpression::Named(NamedExpression::Meta(Meta {
                        key: MetaKey::L4proto,
                    })),
                    NftExpression::Named(NamedExpression::Payload(Payload::PayloadField(
                        PayloadField {
                            protocol: "th".into(),
                            field: "dport".into(),
                        },
                    ))),
                ])),
                data: NftExpression::String(
                    format!(
                        "@{}",
                        self.config.nftables.map_service_chain_by_nodeport.clone()
                    )
                    .into(),
                ),
            }),
        ];
        batch.add(NfListObject::Rule(Rule {
            family: NfFamily::IP,
            table: self.config.nftables.table.clone().into(),
            chain: self.config.nftables.chain_services.clone().into(),
            expr: vmap_to_service_lookup.as_ref().into(),
            comment: Some("Cek IPv4 paket di list IPv4 NodePort, kalo ada langsung ke verdict map ke service yang sesuai".into()),
            handle: Some(0),
            ..Default::default()
        }));

        let ruleset = batch.to_nftables();
        debug!("actor: applying initial ruleset: {ruleset:?}");
        helper::apply_ruleset(&ruleset)?;

        Ok(())
    }
}
