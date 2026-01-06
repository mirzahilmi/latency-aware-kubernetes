use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub service_level_agreement: u128,
    pub exponential_decay_constant: f64,
    pub prometheus: PrometheusConfig,
    pub nftables: NftablesConfig,
    pub kubernetes: KubernetesConfig,
    pub probe: ProbeConfig,
    pub weight: WeightConfig,
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
pub struct WeightConfig {
    pub response_time: f64,
    pub cpu_usage: f64,
}
