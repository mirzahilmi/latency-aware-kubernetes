use lazy_static::lazy_static;
use prometheus::{Encoder, GaugeVec, Opts, Registry, TextEncoder};

lazy_static! {
    static ref REGISTRY: Registry = Registry::new();

    // Gauge per-node (label: node)
    static ref EWMA_CPU: GaugeVec = GaugeVec::new(
        Opts::new("proberv2_ewma_cpu_score", "EWMA-smoothed CPU usage fraction per node"),
        &["node"],
    ).unwrap();
    static ref EWMA_LATENCY: GaugeVec = GaugeVec::new(
        Opts::new("proberv2_ewma_latency_score_ms", "EWMA-smoothed response time in ms per node"),
        &["node"],
    ).unwrap();
    static ref RAW_CPU: GaugeVec = GaugeVec::new(
        Opts::new("proberv2_raw_cpu_usage", "Raw (pre-EWMA) CPU usage fraction per node"),
        &["node"],
    ).unwrap();
    static ref RAW_LATENCY: GaugeVec = GaugeVec::new(
        Opts::new("proberv2_raw_latency_ms", "Raw measured response time in ms per node"),
        &["node"],
    ).unwrap();

    // Gauge per-node per-service (labels: node, service)
    static ref PERFORMANCE_SCORE: GaugeVec = GaugeVec::new(
        Opts::new("proberv2_performance_score", "Raw performance score (1-cpu)/latency per node"),
        &["node", "service"],
    ).unwrap();
    static ref SCORE_PERCENTAGE: GaugeVec = GaugeVec::new(
        Opts::new("proberv2_score_percentage", "Score as percentage of total score per node"),
        &["node", "service"],
    ).unwrap();
    static ref NFT_SLOTS: GaugeVec = GaugeVec::new(
        Opts::new("proberv2_nft_slots", "Number of nftables slots allocated per node"),
        &["node", "service"],
    ).unwrap();
    static ref NODE_ELIGIBLE: GaugeVec = GaugeVec::new(
        Opts::new("proberv2_node_eligible", "Whether node is eligible for traffic (1=yes, 0=no)"),
        &["node", "service"],
    ).unwrap();

    // Gauge per-service (label: service)
    static ref PROBABILITY_CAP: GaugeVec = GaugeVec::new(
        Opts::new("proberv2_probability_cap", "Configured probability cap for slot allocation"),
        &["service"],
    ).unwrap();
}

/// Mendaftarkan semua gauge ke registry. Panggil sekali saat startup.
pub fn init() {
    let collectors: Vec<Box<dyn prometheus::core::Collector>> = vec![
        Box::new(EWMA_CPU.clone()),
        Box::new(EWMA_LATENCY.clone()),
        Box::new(RAW_CPU.clone()),
        Box::new(RAW_LATENCY.clone()),
        Box::new(PERFORMANCE_SCORE.clone()),
        Box::new(SCORE_PERCENTAGE.clone()),
        Box::new(NFT_SLOTS.clone()),
        Box::new(NODE_ELIGIBLE.clone()),
        Box::new(PROBABILITY_CAP.clone()),
    ];
    for c in collectors {
        if let Err(e) = REGISTRY.register(c) {
            tracing::warn!("metrics: gagal mendaftarkan collector: {e}");
        }
    }
}

/// Menghasilkan output metrik dalam format teks Prometheus
pub fn gather() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        tracing::error!("metrics: gagal encode metrik: {e}");
        return String::new();
    }
    String::from_utf8(buffer).unwrap_or_default()
}

// --- Helper setter functions ---

pub fn set_ewma_cpu(node: &str, value: f64) {
    EWMA_CPU.with_label_values(&[node]).set(value);
}

pub fn set_ewma_latency(node: &str, value: f64) {
    EWMA_LATENCY.with_label_values(&[node]).set(value);
}

pub fn set_raw_cpu_usage(node: &str, value: f64) {
    RAW_CPU.with_label_values(&[node]).set(value);
}

pub fn set_raw_latency_ms(node: &str, value: f64) {
    RAW_LATENCY.with_label_values(&[node]).set(value);
}

pub fn set_performance_score(node: &str, service: &str, value: f64) {
    PERFORMANCE_SCORE.with_label_values(&[node, service]).set(value);
}

pub fn set_score_percentage(node: &str, service: &str, value: f64) {
    SCORE_PERCENTAGE.with_label_values(&[node, service]).set(value);
}

pub fn set_nft_slots(node: &str, service: &str, value: u32) {
    NFT_SLOTS.with_label_values(&[node, service]).set(value as f64);
}

pub fn set_node_eligible(node: &str, service: &str, value: f64) {
    NODE_ELIGIBLE.with_label_values(&[node, service]).set(value);
}

pub fn set_probability_cap(service: &str, value: u32) {
    PROBABILITY_CAP.with_label_values(&[service]).set(value as f64);
}
