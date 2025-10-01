use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::http;
use futures::lock::Mutex;
use k8s_openapi::{api::core::v1::Node, apimachinery::pkg::api::resource::Quantity};
use kube::{Api, Client, ResourceExt, api::ListParams, core::Expression};
use serde::Deserialize;
use tracing::{error, info};

type NodeAlloc = std::collections::BTreeMap<String, Quantity>;

#[derive(Debug)]
struct NodeSummary {
    name: String,
    metrics: NodeMetrics,
    allocatable: NodeAlloc,
}

#[derive(Debug, Deserialize)]
struct NodeMetrics {
    cpu: Metric,
    memory: Metric,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Metric {
    #[serde(rename_all = "camelCase")]
    Cpu { usage_nano_cores: usize },

    #[serde(rename_all = "camelCase")]
    Memory { usage_bytes: usize },
}

impl Metric {
    // Convert measurement to what we will use in the table.
    // - CPU values are represented in millicores
    // - Memory values are represented in MiB (mebibyte)
    fn convert_to_stat(&self, alloc_total: usize) -> f64 {
        match self {
            // 1 millicore = 1000th of a CPU, 1 nano core = 1 billionth of a CPU
            // convert nano to milli
            Metric::Cpu { usage_nano_cores } => {
                // 1 millicore is a 1000th of a CPU. Our values are in
                // nanocores (a billionth of a CPU), so convert from nano to
                // milli.
                let cpu_m = (usage_nano_cores / (1000 * 1000)) as f64;
                // Convert a whole core to a millicore value
                let alloc_m = (alloc_total * 1000) as f64;
                // Normalize
                1.0 - cpu_m / alloc_m
            }

            Metric::Memory { usage_bytes } => {
                let mem_mib = *usage_bytes as f64 / (u64::pow(2, 20)) as f64;
                let alloc_mib = alloc_total as f64 / (u64::pow(2, 10)) as f64;
                mem_mib / alloc_mib
            }
        }
    }
}

pub struct CpuCollector {
    pub proc_sleep: Duration,
    pub shutdown_sig: tokio::sync::broadcast::Receiver<()>,
    pub retry_threshold: u32,
    pub kube_client: Client,
    pub ewma_cpu_by_host: Arc<Mutex<HashMap<String, f64>>>,
}

impl CpuCollector {
    pub async fn run(&mut self) {
        info!("cpu_watcher: initialize background process");
        loop {
            let interval = tokio::time::sleep(self.proc_sleep);
            tokio::select! {
                _ = interval => self.try_watch().await,
                _ = self.shutdown_sig.recv() => {
                    info!("cpu_watcher: shutting down: breaking out process");
                    break;
                },
            }
        }
    }

    async fn try_watch(&mut self) {
        let mut attempts = 0;
        while attempts < self.retry_threshold {
            info!(
                "cpu_watcher: attempting collect on {}/{} attempt",
                attempts + 1,
                self.retry_threshold
            );
            let Err(e) = self.collect().await else {
                return;
            };
            error!("cpu_watcher: failed to collect: {e}");
            attempts += 1;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        error!(
            "cpu_watcher: {}/{} attempts failed, continuing to next cycle",
            attempts, self.retry_threshold
        );
    }

    async fn collect(&mut self) -> anyhow::Result<()> {
        let nodes_api = Api::<Node>::all(self.kube_client.clone());
        let exp =
            Expression::DoesNotExist("node-role.kubernetes.io/control-plane".to_string()).into();
        let nodes = nodes_api
            .list(&ListParams::default().labels_from(&exp))
            .await?;

        for node in nodes {
            let name = node.name_any();
            let url = format!("/api/v1/nodes/{}/proxy/stats/summary", name);
            let req = http::Request::get(url).body(Default::default())?;

            // Deserialize JSON response as a JSON value. Alternatively, a type that
            // implements `Deserialize` can be used.
            let resp = self.kube_client.request::<serde_json::Value>(req).await?;

            // Our JSON value is an object so we can treat it like a dictionary.
            let summary = resp
                .get("node")
                .expect("node summary should exist in kubelet's admin endpoint");

            // The base JSON representation includes a lot of metrics, including
            // container metrics. Use a `NodeMetrics` type to deserialize only the
            // values we care about.
            let metrics = serde_json::from_value::<NodeMetrics>(summary.to_owned())?;

            // Get the current allocatable values for the node we are looking at and
            // save in a table we will use to print the results.
            let allocatable = node
                .status
                .unwrap_or_default()
                .allocatable
                .unwrap_or_default();

            let node_summary = NodeSummary {
                name,
                metrics,
                allocatable,
            };

            let cpu_total = node_summary
                .allocatable
                .get("cpu")
                .map(|mem| mem.0.parse::<usize>().ok().unwrap_or(1))
                .unwrap_or_else(|| 1);
            let cpu_normalized = node_summary.metrics.cpu.convert_to_stat(cpu_total);

            {
                let mut ewma_cpu_by_host = self.ewma_cpu_by_host.lock().await;
                let key = node_summary.name.clone();
                let ewma_calculated = match ewma_cpu_by_host.get(&key) {
                    Some(prev_datapoint) => {
                        // should be changed
                        let constant_alpha = 0.5;
                        constant_alpha * cpu_normalized + (1.0 - constant_alpha) * *prev_datapoint
                    }
                    None => cpu_normalized,
                };
                ewma_cpu_by_host.insert(key, ewma_calculated);
            }
        }

        Ok(())
    }
}
