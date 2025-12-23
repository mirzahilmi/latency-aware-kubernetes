use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub service_level_agreement: f64,
    pub exponential_decay_constant: f64,
    #[serde(skip)]
    pub node_name: String,
    pub namespace: String,
    pub prometheus: PrometheusConfig,
    pub nftables: NftablesConfig,
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
