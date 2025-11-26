use std::{borrow::Cow, collections::HashMap};

use nftables::{
    batch::Batch,
    expr::{
        Expression, Map, NamedExpression, NgMode, Numgen, Payload, PayloadField, Range, SetItem,
    },
    helper,
    schema::{Chain, FlushObject, NfCmd, NfListObject, Rule},
    stmt::{Match, NAT, NATFamily, Operator, Statement},
    types::NfFamily,
};
use serde_json::json;
use tracing::{debug, info, warn};

use crate::{
    actor::{ScorePair, Service},
    config::Config,
};

pub async fn update_nftables(
    config: Config,
    service: Service,
    datapoint_by_nodename: HashMap<String, Option<ScorePair>>,
) -> anyhow::Result<()> {
    info!("actor: starting to modify nftables for traffic routing");
    debug!(
        "actor: attempting to apply routing rulesets with args: {service:?}: {datapoint_by_nodename:?}"
    );

    if service.endpoints_by_nodename.len() < 2 {
        info!(
            "actor: skipping ruleset application for service {} that only has {} nodes distribution",
            service.name,
            service.endpoints_by_nodename.len()
        );
        return Ok(());
    }

    let chain = format!(
        "{}-{}",
        config.nftables.prefix_service_endpoint, service.name
    );

    let mut batch = Batch::new();
    batch.add_cmd(NfCmd::Flush(FlushObject::Chain(Chain {
        family: NfFamily::IP,
        table: config.nftables.table.clone().into(),
        name: chain.clone().into(),
        ..Default::default()
    })));
    let ruleset = batch.to_nftables();
    let _ = helper::apply_ruleset(&ruleset); // ignoring chain flush error

    let mut batch = Batch::new();
    batch.delete(NfListObject::Chain(Chain {
        family: NfFamily::IP,
        table: config.nftables.table.clone().into(),
        name: chain.clone().into(),
        ..Default::default()
    }));
    let ruleset = batch.to_nftables();
    let _ = helper::apply_ruleset(&ruleset); // ignoring chain deletion error

    let mut batch = Batch::new();
    batch.add(NfListObject::Chain(Chain {
        family: NfFamily::IP,
        table: config.nftables.table.clone().into(),
        name: chain.clone().into(),
        ..Default::default()
    }));
    let ruleset = batch.to_nftables();
    debug!(
        "actor: creating base service chain: {}",
        serde_json::to_string(&ruleset)?
    );
    helper::apply_ruleset(&ruleset)?;

    let mut total_endpoints = 0;
    let mut total_datapoints = 0.0;

    service
        .endpoints_by_nodename
        .iter()
        .for_each(|(nodename, endpoints)| {
            let Some(datapoint) = datapoint_by_nodename
                .get(nodename)
                .and_then(|datapoint| datapoint.as_ref())
            else {
                warn!("actor: skipping nodename {nodename} that still does not have datapoint");
                return;
            };
            total_endpoints += endpoints.len();
            total_datapoints += datapoint.latency + datapoint.cpu;
        });

    if total_endpoints < 2 {
        warn!(
            "actor: skipping distributed service {} with only {total_endpoints} endpoints",
            service.name,
        );
        return Ok(());
    } else if total_datapoints < 1.0 {
        warn!("actor: skipping total datapoints only results into {total_datapoints}",);
        return Ok(());
    }

    let mut verdict_pairs = Vec::<SetItem>::new();
    let mut starting = 0;
    service
        .endpoints_by_nodename
        .iter()
        .for_each(|(nodename, endpoints)| {
            let Some(datapoint) = datapoint_by_nodename
                .get(nodename)
                .and_then(|datapoint| datapoint.as_ref())
            else {
                return;
            };
            let score_percentage = (datapoint.latency + datapoint.cpu) / total_datapoints;
            let node_portion = score_percentage * config.nftables.probability_cap as f64;
            // scary
            let portion_each = (node_portion / endpoints.len() as f64) as u32;

            endpoints.iter().for_each(|endpoint| {
                verdict_pairs.push(SetItem::Mapping(
                    Expression::Range(
                        Range {
                            range: [
                                Expression::Number(starting),
                                Expression::Number(starting + portion_each),
                            ],
                        }
                        .into(),
                    ),
                    Expression::String(endpoint.into()),
                ));
                starting += portion_each + 1;
            });
        });

    let mut batch = Batch::new();
    batch.add(NfListObject::Rule(Rule {
        family: NfFamily::IP,
        table: config.nftables.table.clone().into(),
        chain: chain.clone().into(),
        expr: Cow::Owned(vec![
            Statement::Match(Match {
                left: Expression::Named(NamedExpression::Payload(Payload::PayloadField(
                    PayloadField {
                        protocol: Cow::Borrowed("tcp"),
                        field: Cow::Borrowed("dport"),
                    },
                ))),
                // scary
                right: Expression::Number(service.nodeport as u32),
                op: Operator::EQ,
            }),
            Statement::DNAT(Some(NAT {
                family: NATFamily::IP.into(),
                addr: Expression::Named(NamedExpression::Map(Box::new(Map {
                    key: Expression::Named(NamedExpression::Numgen(Numgen {
                        mode: NgMode::Random,
                        ng_mod: starting - 1,
                        ..Default::default()
                    })),
                    data: Expression::Named(NamedExpression::Set(verdict_pairs)),
                })))
                .into(),
                // scary
                port: Some(Expression::Number(service.targetport as u32)),
                flags: None,
            })),
        ]),
        comment: Some(format!("Ini chains buat load balancing service {}", chain).into()),
        handle: Some(0),
        ..Default::default()
    }));

    let ruleset = batch.to_nftables();
    debug!(
        "actor: attaching anonymous map routing rule: {}",
        serde_json::to_string(&ruleset)?
    );
    helper::apply_ruleset(&ruleset)?;

    // used raw because the crate does not has map type of `verdict`
    helper::apply_ruleset_raw(
        json!({
          "nftables": [
            {
              "add": {
                "element": {
                  "family": "ip",
                  "table": config.nftables.table,
                  "name": config.nftables.map_service_chain_by_nodeport,
                  "elem": [
                    [
                      {
                        "concat": [
                          "tcp",
                          service.nodeport
                        ]
                      },
                      {
                        "goto": {
                          "target": chain
                        }
                      }
                    ]
                  ]
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

    Ok(())
}
