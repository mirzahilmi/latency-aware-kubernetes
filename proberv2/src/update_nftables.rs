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
    mut service: Service,
    datapoint_by_nodename: HashMap<String, Option<ScorePair>>,
) -> anyhow::Result<()> {
    // skip nft update if service only has LESS THAN 2 node
    if service.endpoints_by_nodename.len() < 2 {
        info!(
            "actor: skipping ruleset application for service {} that only has {} nodes distribution",
            service.name,
            service.endpoints_by_nodename.len()
        );
        return Ok(());
    }

    info!("actor: starting to modify nftables for traffic routing");
    debug!(
        "actor: attempting to apply routing rulesets with args: {service:?}: {datapoint_by_nodename:?}"
    );

    let chain = format!(
        "{}-{}",
        config.nftables.prefix_service_endpoint, service.name
    );

    let mut total_endpoints = 0;
    let mut total_score = 0.0;

    // filters out node with no datapoint and 0% cpu availability
    service.endpoints_by_nodename.retain(|nodename, _| {
        let Some(datapoint) = datapoint_by_nodename
            .get(nodename)
            .and_then(|datapoint| datapoint.as_ref())
        else {
            return false;
        };
        datapoint.cpu > 0.0 && datapoint.cpu < 0.95
    });

    // count total endpoints and score
    service
        .endpoints_by_nodename
        .iter()
        .for_each(|(nodename, endpoints)| {
            let datapoint = datapoint_by_nodename
                .get(nodename)
                .and_then(|datapoint| datapoint.as_ref())
                // this should be safe right, above code already filters out nodename with no datapoint
                .unwrap();

            let score = (1.0 - datapoint.cpu) / datapoint.latency;
            total_endpoints += endpoints.len();
            total_score += score;
        });

    if total_endpoints < 2 {
        warn!(
            "actor: skipping distributed service {} with only {total_endpoints} endpoints",
            service.name,
        );
        return Ok(());
    }

    let mut verdict_pairs = Vec::<SetItem>::new();
    let mut starting = 0u32;
    let probability_cap = config.nftables.probability_cap;
    let mut score_by_nodename = HashMap::new();

    service
        .endpoints_by_nodename
        .iter()
        .for_each(|(nodename, endpoints)| {
            let datapoint = datapoint_by_nodename
                .get(nodename)
                .and_then(|datapoint| datapoint.as_ref())
                // this should be safe right, above code already filters out nodename with no datapoint
                .unwrap();

            let score = (1.0 - datapoint.cpu) / datapoint.latency;
            let score_percentage = score / total_score;

            score_by_nodename.insert(nodename.clone(), score_percentage * 100.0);
            let node_portion = (score_percentage * probability_cap as f64).round() as u32;

            if node_portion == 0 {
                warn!("actor: node {} got 0 portion, skipping", nodename);
                return;
            }

            // Distribute evenly across endpoints, using floor to stay within bounds
            let portion_each = node_portion / endpoints.len() as u32;
            let remainder = node_portion % endpoints.len() as u32;

            if portion_each == 0 {
                warn!(
                    "actor: portion_each is 0 for node {} with {} endpoints",
                    nodename,
                    endpoints.len()
                );
                return;
            }

            for (idx, endpoint) in endpoints.iter().enumerate() {
                // Give remainder to first few endpoints
                let this_portion = if idx < remainder as usize {
                    portion_each + 1
                } else {
                    portion_each
                };

                // Safety check: don't exceed probability_cap
                if starting >= probability_cap {
                    warn!(
                        "actor: reached probability_cap limit, stopping at {}",
                        starting
                    );
                    return;
                }

                let end = (starting + this_portion - 1).min(probability_cap - 1);

                verdict_pairs.push(SetItem::Mapping(
                    Expression::Range(
                        Range {
                            range: [Expression::Number(starting), Expression::Number(end)],
                        }
                        .into(),
                    ),
                    Expression::String(endpoint.to_string().into()),
                ));
                starting = end + 1;

                if starting >= probability_cap {
                    break;
                }
            }
        });
    info!("actor: {chain} node scores: {score_by_nodename:?}");

    if verdict_pairs.is_empty() {
        warn!(
            "actor: no verdict pairs generated for service {}, skipping",
            service.name
        );
        return Ok(());
    }

    let ng_mod_value = if starting > 0 {
        starting - 1
    } else {
        probability_cap - 1
    };

    debug!(
        "actor: generated {} verdict pairs, range coverage: [0, {}], ng_mod: {}",
        verdict_pairs.len(),
        starting - 1,
        ng_mod_value
    );

    // try create service chain first, if already exist just error silently
    let mut batch = Batch::new();
    batch.add(NfListObject::Chain(Chain {
        family: NfFamily::IP,
        table: config.nftables.table.clone().into(),
        name: chain.clone().into(),
        ..Default::default()
    }));
    helper::apply_ruleset(&batch.to_nftables())?;

    let mut batch = Batch::new();
    batch.add_cmd(NfCmd::Flush(FlushObject::Chain(Chain {
        family: NfFamily::IP,
        table: config.nftables.table.clone().into(),
        name: chain.clone().into(),
        ..Default::default()
    })));
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
                right: Expression::Number(service.nodeport as u32),
                op: Operator::EQ,
            }),
            Statement::DNAT(Some(NAT {
                family: NATFamily::IP.into(),
                addr: Expression::Named(NamedExpression::Map(Box::new(Map {
                    key: Expression::Named(NamedExpression::Numgen(Numgen {
                        mode: NgMode::Random,
                        ng_mod: ng_mod_value,
                        ..Default::default()
                    })),
                    data: Expression::Named(NamedExpression::Set(verdict_pairs)),
                })))
                .into(),
                port: Some(Expression::Number(service.targetport as u32)),
                flags: None,
            })),
        ]),
        comment: Some(format!("Load balancing for service {}", chain).into()),
        handle: Some(0),
        ..Default::default()
    }));

    let ruleset = batch.to_nftables();
    debug!(
        "actor: attaching anonymous map routing rule: {}",
        serde_json::to_string(&ruleset)?
    );
    helper::apply_ruleset(&ruleset)?;

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
