use axum::{extract::State, Json};
use std::sync::Arc;
use tracing::{info, warn, debug, trace};
use crate::{models::*, state::AppState};

// Thresholds (pakai skala 0..1 dari EWMA)
const CPU_HARD_LIMIT: f64 = 0.85;       // >85% dianggap overload
const LATENCY_HARD_LIMIT: f64 = 0.50;   // contoh angka (sesuaikan dengan skala kamu)

pub async fn filter_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExtenderArgs>,
) -> Json<FilterResult> {
    info!("=== FILTER PHASE STARTED ===");
    debug!("Raw payload: {:?}", payload);

    let candidate_nodes: Vec<Node> = if !payload.nodes.items.is_empty() {
        payload.nodes.items.into_iter()
            .filter(|n| n.metadata.name.is_some())
            .collect()
    } else if let Some(names) = payload.node_names.clone() {
        names.into_iter().map(|n| Node {
            metadata: Metadata { name: Some(n) }
        }).collect()
    } else {
        state.get_monitored_nodes().await.into_iter().map(|hostname| Node {
            metadata: Metadata { name: Some(hostname) }
        }).collect()
    };

    let mut filtered: Vec<Node> = Vec::new();
    let mut failed_nodes = std::collections::HashMap::new();

    for node in candidate_nodes.into_iter() {
        let node_name = node.metadata.name.clone().unwrap_or_default();
        if node_name.is_empty() { continue; }

        match state.get_probe(&node_name).await {
            Some(probe) => {
                let cpu = probe.cpu_ewma_score;
                let latency = probe.latency_ewma_score;

                trace!("Evaluating node={} → CPU={:.3}, Latency={:.3}", node_name, cpu, latency);

                if cpu > CPU_HARD_LIMIT || latency > LATENCY_HARD_LIMIT {
                    let reason = format!(
                        "Over threshold: CPU={:.3} (limit {:.2}), Latency={:.3} (limit {:.2})",
                        cpu, CPU_HARD_LIMIT, latency, LATENCY_HARD_LIMIT
                    );
                    warn!("Node {} FILTERED OUT: {}", node_name, reason);
                    failed_nodes.insert(node_name.clone(), reason);
                } else {
                    info!("Node {} PASSED filter", node_name);
                    filtered.push(node);
                }
            }
            None => {
                let reason = "No probe data available".to_string();
                warn!("Node {} FILTERED OUT: {}", node_name, reason);
                failed_nodes.insert(node_name.clone(), reason);
            }
        }
    }

    state.update_last_filtered(
        filtered.iter().filter_map(|n| n.metadata.name.clone()).collect()
    ).await;

    info!("=== FILTER PHASE COMPLETED === ({} passed, {} failed)", filtered.len(), failed_nodes.len());

    Json(FilterResult {
        nodes: NodeList { items: filtered, metadata: None },
        failed_nodes,
    })
}
