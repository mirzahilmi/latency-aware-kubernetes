use crate::models::{Probe, PrometheusResponse};
use crate::kube_client::get_prober_mapping;
use std::collections::HashMap;
use tracing::{info, warn, trace};
use std::sync::Arc;
use tokio::sync::RwLock;
use reqwest::Client;

#[derive(Clone, Debug)]
pub struct AppState {
    pub probes: Arc<RwLock<HashMap<String, Probe>>>, // key = hostname/node
    pub pod_counts: Arc<RwLock<HashMap<String, usize>>>,
    pub last_filtered: Arc<RwLock<Vec<String>>>,
    pub local_node: Arc<RwLock<String>>,
    pub namespace: String,
    pub prober_mapping: Arc<RwLock<HashMap<String, String>>>,
    prometheus_url: String,
    http_client: Client,
}

impl AppState {
    pub fn new() -> Self {
        let prometheus_url = std::env::var("PROMETHEUS_URL")
            .unwrap_or_else(|_| "http://prometheus.monitoring.svc.cluster.local:9090".to_string());
        let ns = std::env::var("PROBER_NAMESPACE").unwrap_or_else(|_| "riset".to_string());

        Self {
            probes: Arc::new(RwLock::new(HashMap::new())),
            pod_counts: Arc::new(RwLock::new(HashMap::new())),
            last_filtered: Arc::new(RwLock::new(Vec::new())),
            local_node: Arc::new(RwLock::new(String::new())),
            prometheus_url,
            http_client: Client::new(),
            namespace: ns,
            prober_mapping: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // used in FILTER/PRIORITIZE phase to see the node value
    pub async fn get_probe(&self, hostname: &str) -> Option<Probe> {
        let probes_map = self.probes.read().await;
        probes_map.get(hostname).cloned()
    }

    // list of monitored nodes (by cache `probes`)
    pub async fn get_monitored_nodes(&self) -> Vec<String> {
        let probes_map = self.probes.read().await;
        probes_map.keys().cloned().collect()
    }

    // Refresh mapping nodeName -> prober-pod FQDN from Kubernetes API (kube-rs)
    pub async fn refresh_prober_mapping(&self) -> anyhow::Result<()> {
        let map = get_prober_mapping(&self.namespace).await?;
        let mut guard = self.prober_mapping.write().await;
        *guard = map;
        info!("Refreshed prober mapping ({} entries)", guard.len());
        Ok(())
    }

    //get FQDN prober for a node, if its return nothing, try to refresh once
    async fn lookup_prober_host_for(&self, node: &str) -> anyhow::Result<String> {
        {
            let map = self.prober_mapping.read().await;
            if let Some(h) = map.get(node) {
                return Ok(h.clone());
            }
        }
        
        self.refresh_prober_mapping().await?;
        let map = self.prober_mapping.read().await;
        map.get(node)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No prober mapping for node {}", node))
    }

    // get busiest node from promtheus, then fetch /scores value from prober pod
    pub async fn update_local_node_and_scores(&self) -> anyhow::Result<()> {
        let query = std::env::var("PROM_QUERY")
            .unwrap_or_else(|_| {
                r#"topk(1, sum by (node) (rate(traefik_entrypoint_requests_total{entrypoint="web"}[1m])))"#.to_string()
            });

        let url = format!(
            "{}/api/v1/query?query={}",
            self.prometheus_url,
            urlencoding::encode(&query)
        );

        let response = self.http_client
            .get(&url)
            .send()
            .await?
            .json::<PrometheusResponse>()
            .await?;
        info!("Prometheus query raw data: {:?}", response.data.result);

        let maybe_node = response.data.result.into_iter()
            .filter_map(|r| r.metric.get("node").cloned())
            .filter(|n| !(n.contains("control-plane") || n.contains("master")))
            .next();

        let Some(node) = maybe_node else {
            warn!("Prometheus query returned no usable results (all filtered out); keep previous local_node");
            return Ok(());
        };

        {
            let mut ln = self.local_node.write().await;
            *ln = node.clone();
        }
        info!("Updated busiest(local_node) → {}", node);

        // FQDN prober pod for the local node
        let host = self.lookup_prober_host_for(&node).await?;
        let url_scores = format!("http://{}/scores", host);
        info!("Fetching probe data from {}", url_scores);

                let new_probe_data = match self.http_client
            .get(&url_scores)
            .send()
            .await?
            .json::<Vec<Probe>>()
            .await
        {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to fetch probe data from {}: {}", url_scores, e);
                return Ok(()); // old caches are used
            }
        };

        if new_probe_data.is_empty() {
            warn!("Fetched probe data from {} but got empty list; keeping old cache", url_scores);
            return Ok(()); // jangan hapus data lama
        }

        // update cache if the data are valid
        let mut probes = self.probes.write().await;
        *probes = new_probe_data
            .into_iter()
            .map(|p| (p.hostname.clone(), p))
            .collect();

        info!("Updated probes cache from {} ({} entries)", node, probes.len());
        Ok(())
    }

    /// periodic loop: refresh mapping + get busiest node + refresh scores
    pub async fn periodic_update_loop(&self) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(e) = self.update_local_node_and_scores().await {
                tracing::warn!("Periodic update failed: {}", e);
            } else {
                tracing::debug!("Periodic update OK");
            }
        }
        // never reached
        #[allow(unreachable_code)]
        Ok(())
    }

    // count how many pods that assigned into a node --> the choosen node will get the cpu penalty
    pub async fn get_pod_count_on_node(&self, node_name: &str) -> usize {
        let counts = self.pod_counts.read().await;
        *counts.get(node_name).unwrap_or(&0)
    }

    pub async fn increment_pod_count(&self, node_name: &str) {
        let mut counts = self.pod_counts.write().await;
        let entry = counts.entry(node_name.to_string()).or_insert(0);
        *entry += 1;
        trace!("Increment pod count for {} → {}", node_name, *entry);
    }

    pub async fn update_last_filtered(&self, nodes: Vec<String>) {
        let mut last = self.last_filtered.write().await;
        *last = nodes;
    }

    pub async fn get_last_filtered_nodes(&self) -> Vec<String> {
        let last = self.last_filtered.read().await;
        last.clone()
    }

    // cpu penalty for the choosen node
    pub async fn apply_cpu_penalty(&self, node_name: &str, penalty: f64) {
        let mut probes_map = self.probes.write().await;
        if let Some(probe) = probes_map.get_mut(node_name) {
            let before = probe.cpu_ewma_score;
            probe.cpu_ewma_score += penalty;
            info!(
                "Applied CPU penalty for {}: {:.3} → {:.3}",
                node_name,
                before,
                probe.cpu_ewma_score
            );
        } else {
            warn!("Tried to apply CPU penalty but no probe found for {}", node_name);
        }
    }
}
