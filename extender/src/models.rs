use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Kubernetes Node Models
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Node {
    pub metadata: Metadata,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Metadata {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct NodeList {
    pub items: Vec<Node>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<k8s_openapi::apimachinery::pkg::apis::meta::v1::ListMeta>,
}

// Scheduler Extender API Models
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ExtenderArgs {
    pub pod: Option<k8s_openapi::api::core::v1::Pod>,
    pub nodes: NodeList,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_names: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FilterResult {
    pub nodes: NodeList,
    #[serde(rename = "failedNodes")]
    pub failed_nodes: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HostPriority {
    #[serde(rename = "Host")]
    pub host: String,
    #[serde(rename = "Score")]
    pub score: i64,
}

// #[derive(Debug, Deserialize, Serialize, Clone)]
// pub struct PrioritizeResult {
//     #[serde(rename = "hostPriorities")]
//     pub host_priorities: Vec<HostPriority>,
// }

// Prometheus response models
#[derive(Debug, Deserialize)]
pub struct PrometheusResponse {
    status: String,
    pub data: PrometheusData,
}

#[derive(Debug, Deserialize)]
pub struct PrometheusData {
    pub result: Vec<PrometheusResult>,
}

#[derive(Debug, Deserialize)]
pub struct PrometheusResult {
    pub metric: HashMap<String, String>,
    pub value: (f64, String),
}

// #[derive(Debug, Deserialize)]
// pub struct PrometheusMetric {
//     pub node: String,
// }

// Probe Data Models
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Probe {
    pub hostname: String,
    #[serde(rename = "cpuEwmaScore")]
    pub cpu_ewma_score: f64,
    #[serde(rename = "latencyEwmaScore")]
    pub latency_ewma_score: f64,
}

impl Probe {
    // node are healthy if node score >= threshold
    pub fn meets_thresholds(&self, cpu_max: f64, latency_max: f64) -> bool {
        self.cpu_ewma_score <= cpu_max && self.latency_ewma_score <= latency_max
    }

    /// ✅ Semakin kecil score semakin bagus → dibalik (1 - normalized)
    // little score are better, so the 
    pub fn calculate_combined_score(&self, cpu_weight: f64, latency_weight: f64) -> f64 {
        let cpu_component = (1.0 - self.cpu_ewma_score).clamp(0.0, 1.0);
        let latency_component = (1.0 - self.latency_ewma_score).clamp(0.0, 1.0);
        (cpu_component * cpu_weight) + (latency_component * latency_weight)
    }

    pub fn to_scheduler_score(&self, cpu_weight: f64, latency_weight: f64) -> i64 {
        let combined = self.calculate_combined_score(cpu_weight, latency_weight);
        (combined * 100.0).round() as i64
    }
}
