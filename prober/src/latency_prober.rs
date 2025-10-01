use anyhow::{Result, anyhow};
use futures::lock::Mutex;
use k8s_openapi::{
    api::core::v1::Node,
    serde::{Deserialize, Serialize},
};
use kube::{Api, Client, ResourceExt, api::ListParams, core::Expression};
use regex::Regex;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::time;
use tokio::{process::Command, task::JoinSet};
use tracing::{error, info};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProbeTarget {
    pub hostname: String,
    pub ip: String,
}

pub struct LatencyProber {
    pub proc_sleep: Duration,
    pub shutdown_sig: tokio::sync::broadcast::Receiver<()>,
    pub retry_threshold: u32,
    pub ping_count: u32,
    pub kube_client: Client,
    pub ewma_latency_by_host: Arc<Mutex<HashMap<String, f64>>>,
}

impl LatencyProber {
    pub async fn run(&mut self) {
        info!("prober: initialize background process");
        loop {
            let interval = tokio::time::sleep(self.proc_sleep);
            tokio::select! {
                _ = interval => self.try_probe().await,
                _ = self.shutdown_sig.recv() => {
                    info!("prober: shutting down: breaking out process");
                    break;
                },
            }
        }
    }

    async fn try_probe(&mut self) {
        let mut attempts = 0;
        while attempts < self.retry_threshold {
            info!(
                "prober: attempting ping on {}/{} attempt",
                attempts + 1,
                self.retry_threshold
            );
            let Err(e) = self.probe().await else {
                return;
            };
            error!("prober: failed to probe: {e}");
            attempts += 1;
            time::sleep(Duration::from_secs(1)).await;
        }
        error!(
            "prober: {}/{} attempts failed, continuing to next cycle",
            attempts, self.retry_threshold
        );
    }

    async fn probe(&mut self) -> anyhow::Result<()> {
        // retrieve all worker nodes
        let nodes_api = Api::<Node>::all(self.kube_client.clone());
        let matcher = ListParams::default().labels_from(
            &Expression::DoesNotExist("node-role.kubernetes.io/control-plane".to_string()).into(),
        );
        let nodes = nodes_api.list(&matcher).await?;
        let targets_map: HashMap<_, _> = nodes
            .iter()
            .filter_map(|node| {
                let status = node.status.as_ref()?;
                let addresses = status.addresses.as_ref()?;
                let address = addresses.iter().find(|addr| addr.type_ == "InternalIP")?;
                Some((address.address.clone(), node.name_any()))
            })
            .collect();
        info!("prober: received probing targets: {:?}", targets_map.keys());

        // ping each host
        let results = ping(targets_map.keys().cloned().collect(), self.ping_count).await?;
        for (target, latency) in results.into_iter() {
            let Some(hostname) = targets_map.get(&target) else {
                continue;
            };
            let hostname = hostname.to_string();

            // calculate EWMA latency
            // 500ms assumed max
            let latency_normalized = 1.0 - latency / 500.0;
            {
                let mut ewma_latency_by_host = self.ewma_latency_by_host.lock().await;
                let ewma_calculated = match ewma_latency_by_host.get(&hostname) {
                    Some(prev_datapoint) => {
                        let constant_alpha = 0.4;
                        constant_alpha * latency_normalized
                            + (1.0 - constant_alpha) * *prev_datapoint
                    }
                    None => latency_normalized,
                };
                ewma_latency_by_host.insert(hostname, ewma_calculated);
            }
        }
        Ok(())
    }
}

// very long & working code :/
async fn ping(targets: Vec<String>, n: u32) -> Result<HashMap<String, f64>> {
    let mut tasks = JoinSet::new();
    let regex = Regex::new(r"(?m)(?:time=){1}(.+)(?:\sms){1}$")?;

    for target in targets {
        let regex = regex.clone();
        tasks.spawn(async move {
            let output = Command::new("ping")
                .arg("-c")
                .arg(format!("{}", &n))
                .arg(&target)
                .output()
                .await?;

            if !output.status.success() {
                return Err(anyhow!(
                    "ping failed for {}: {}",
                    target,
                    String::from_utf8_lossy(&output.stderr)
                ));
            }

            let out = String::from_utf8(output.stdout)?;

            let mut metrics = vec![];
            for (_, [metric_str]) in regex.captures_iter(&out).map(|c| c.extract()) {
                if let Ok(metric) = metric_str.parse::<f32>() {
                    metrics.push(metric);
                }
            }

            if metrics.is_empty() {
                return Err(anyhow!("no ping metrics found for {}", target));
            }

            let avg = metrics.iter().sum::<f32>() as f64 / metrics.len() as f64;

            Ok((target, avg))
        });
    }

    let mut results = HashMap::new();
    while let Some(join_result) = tasks.join_next().await {
        match join_result {
            Ok(Ok((target, avg))) => {
                results.insert(target, avg);
            }
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(e.into()),
        }
    }

    Ok(results)
}
