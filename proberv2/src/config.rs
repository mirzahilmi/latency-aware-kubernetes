use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub shutdown_timeout: u32,
    pub prometheus: PrometheusConfig,
    pub nftables: NftablesConfig,
    pub kubernetes: KubernetesConfig,
    pub probe: ProbeConfig,
    pub alpha: AlphaConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PrometheusConfig {
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NftablesConfig {
    pub table: String,
    pub chain_prerouting: String,
    pub chain_services: String,
    pub set_allowed_node_ips: String,
    pub map_service_chain_by_nodeport: String,
    pub prefix_service_endpoint: String,
    pub probability_cap: u32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesConfig {
    pub namespace: String,
    #[serde(skip)]
    pub node_name: String,
    pub service: String,
    pub target_port: u32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProbeConfig {
    pub latency_interval: u64,
    pub cpu_interval: u64,
    pub nft_update_interval: u64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlphaConfig {
    pub ewma_latency: f64,
    pub ewma_cpu: f64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MetricsConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_listen_addr() -> String {
    "0.0.0.0:9101".to_string()
}

fn default_enabled() -> bool {
    true
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            enabled: default_enabled(),
        }
    }
}
