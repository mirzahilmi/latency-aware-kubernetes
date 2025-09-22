use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use futures::lock::Mutex;
use nftables::{
    batch::Batch,
    expr::{Expression, NamedExpression, NgMode, Numgen, Range, SetItem, Verdict},
    schema::{NfCmd, Rule},
    stmt::{JumpTarget, Statement, VerdictMap},
    types::NfFamily,
};
use tracing::{error, info};

use crate::nftables_watcher::NftablesService;

pub struct NftablesBalancer {
    pub proc_sleep: Duration,
    pub shutdown_sig: tokio::sync::broadcast::Receiver<()>,
    pub retry_threshold: u32,
    pub nftables_chain_by_service: Arc<Mutex<HashMap<String, NftablesService>>>,
    pub ewma_latency_by_host: Arc<Mutex<HashMap<String, f64>>>,
    pub ewma_cpu_by_host: Arc<Mutex<HashMap<String, f64>>>,
}

impl NftablesBalancer {
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
            attempts + 1,
            self.retry_threshold
        );
    }

    async fn reconcile(&mut self) -> anyhow::Result<()> {
        // acquire lock early to prevent all other process updates
        let nftables = self.nftables_chain_by_service.lock().await;
        let mut ewma_cpu_lookup = self.ewma_cpu_by_host.lock().await;
        let mut ewma_latency_lookup = self.ewma_latency_by_host.lock().await;

        let mut hostnames: HashSet<String> = HashSet::new();
        nftables.iter().for_each(|(_, service)| {
            hostnames.extend(service.endpoints_lookup.clone().into_keys())
        });

        ewma_cpu_lookup.retain(|hostname, _| hostnames.contains(hostname));
        ewma_latency_lookup.retain(|hostname, _| hostnames.contains(hostname));

        let mut hostname_scores_lookup = HashMap::new();
        hostnames.iter().for_each(|hostname| {
            let Some(ewma_cpu) = ewma_cpu_lookup.get(hostname) else {
                return;
            };
            let Some(ewma_latency) = ewma_latency_lookup.get(hostname) else {
                return;
            };
            hostname_scores_lookup.insert(hostname.to_owned(), ewma_cpu + ewma_latency);
        });

        let total_score: f64 = hostname_scores_lookup.values().sum();
        hostname_scores_lookup.values_mut().for_each(|score| {
            *score /= total_score;
        });

        // let mut
        info!("NftablesUpdate: {:?}", nftables);

        let mut batch = Batch::new();
        nftables.iter().for_each(|(service_name, service)| {
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
                        ng_mod: 2,
                        offset: None,
                    })),
                    data: Expression::Named(NamedExpression::Set(vec![
                        SetItem::Element(Expression::Range(
                            Range {
                                range: [Expression::Number(0), Expression::Number(0)],
                            }
                            .into(),
                        )),
                        SetItem::Element(Expression::Verdict(Verdict::Goto(JumpTarget {
                            target: "".into(),
                        }))),
                    ])),
                })]
                .into(),
            }));
        });

        Ok(())
    }
}
