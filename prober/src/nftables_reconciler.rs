use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use futures::lock::Mutex;
use nftables::{
    batch::Batch,
    expr::{Expression, NamedExpression, NgMode, Numgen, Range, SetItem, Verdict},
    helper,
    schema::{NfCmd, Rule},
    stmt::{JumpTarget, Statement, VerdictMap},
    types::NfFamily,
};
use tracing::{error, info};

use crate::nftables_watcher::NftablesService;

pub struct NftablesReconciler {
    pub proc_sleep: Duration,
    pub shutdown_sig: tokio::sync::broadcast::Receiver<()>,
    pub retry_threshold: u32,
    pub nftables_chain_by_service: Arc<Mutex<HashMap<String, NftablesService>>>,
    pub ewma_latency_by_host: Arc<Mutex<HashMap<String, f64>>>,
    pub ewma_cpu_by_host: Arc<Mutex<HashMap<String, f64>>>,
}

impl NftablesReconciler {
    pub async fn run(&mut self) {
        loop {
            let interval = tokio::time::sleep(self.proc_sleep);
            tokio::select! {
                _ = interval => self.try_reconcile().await,
                _ = self.shutdown_sig.recv() => {
                    info!("nftables_balancer: shutting down: breaking out process");
                    break;
                },
            }
        }
    }

    async fn try_reconcile(&mut self) {
        let mut attempts = 0;
        while attempts < self.retry_threshold {
            info!(
                "nftables_balancer: attempting reconcile on {}/{} attempt",
                attempts + 1,
                self.retry_threshold
            );
            let Err(e) = self.reconcile().await else {
                return;
            };
            error!("nftables_balancer: failed to reconcile: {e}");
            attempts += 1;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        error!(
            "nftables_balancer: {}/{} attempts failed, continuing to next cycle",
            attempts, self.retry_threshold
        );
    }

    async fn reconcile(&mut self) -> anyhow::Result<()> {
        // acquire lock early to prevent all other process updates
        let nftables_chain_by_service = self.nftables_chain_by_service.lock().await;
        let mut ewma_cpu_by_host = self.ewma_cpu_by_host.lock().await.clone();
        let mut ewma_latency_by_host = self.ewma_latency_by_host.lock().await.clone();

        let mut hostnames: HashSet<String> = HashSet::new();
        nftables_chain_by_service.iter().for_each(|(_, service)| {
            hostnames.extend(service.endpoints_by_host.clone().into_keys())
        });

        ewma_cpu_by_host.retain(|hostname, _| hostnames.contains(hostname));
        ewma_latency_by_host.retain(|hostname, _| hostnames.contains(hostname));

        let mut hostname_scores_lookup = HashMap::new();
        hostnames.iter().for_each(|hostname| {
            let Some(ewma_cpu) = ewma_cpu_by_host.get(hostname) else {
                return;
            };
            let Some(ewma_latency) = ewma_latency_by_host.get(hostname) else {
                return;
            };
            hostname_scores_lookup.insert(hostname.to_owned(), ewma_cpu + ewma_latency);
        });

        let total_score: f64 = hostname_scores_lookup.values().sum();
        hostname_scores_lookup.values_mut().for_each(|score| {
            *score /= total_score;
        });

        info!("NftablesUpdate: {:?}", nftables_chain_by_service);
        if hostname_scores_lookup.is_empty() {
            return Ok(());
        }

        let mut batch = Batch::new();
        nftables_chain_by_service
            .iter()
            .for_each(|(service_name, service)| {
                // distribute weight to backends based on host score
                let mut backend_verdicts = vec![];
                let mut start_range: u32 = 0;
                service
                    .endpoints_by_host
                    .iter()
                    .for_each(|(hostname, backends)| {
                        let Some(portion_percentage) = hostname_scores_lookup.get(hostname) else {
                            return;
                        };
                        let portion = start_range as f64 + portion_percentage - 1.0;
                        let portion_each = (portion / backends.len() as f64).round() as u32;
                        backends.iter().for_each(|backend| {
                            backend_verdicts.push(SetItem::Mapping(
                                Expression::Range(
                                    Range {
                                        range: [
                                            Expression::Number(start_range),
                                            Expression::Number(start_range + portion_each),
                                        ],
                                    }
                                    .into(),
                                ),
                                Expression::Verdict(Verdict::Goto(JumpTarget {
                                    target: backend.into(),
                                })),
                            ));
                            start_range += portion_each;
                        });
                    });

                let comment = format!("Probabilistic Load-Balancing for Service {service_name}");
                batch.add_cmd(NfCmd::Replace(Rule {
                    family: NfFamily::IP,
                    table: "kube-proxy".into(),
                    chain: service.id.clone().into(),
                    handle: service.vmap_handle.into(),
                    comment: Some(comment.into()),
                    index: None,
                    expr: vec![Statement::VerdictMap(VerdictMap {
                        key: Expression::Named(NamedExpression::Numgen(Numgen {
                            mode: NgMode::Random,
                            ng_mod: 100,
                            offset: None,
                        })),
                        data: Expression::Named(NamedExpression::Set(backend_verdicts)),
                    })]
                    .into(),
                }));
            });

        let nftables = batch.to_nftables();
        info!("nftables_balancer: applying rule: {nftables:?}");
        helper::apply_ruleset(&nftables)?;

        Ok(())
    }
}
