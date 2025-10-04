use axum::{extract::State, Json};
use tracing::{info, debug, trace};
use crate::models::{ExtenderArgs, HostPriority, PrioritizeResult};
use crate::state::AppState;
use std::sync::Arc;

const CPU_WEIGHT: f64 = 0.3;
const LATENCY_WEIGHT: f64 = 0.7;
const PENALTY_CPU_RANGE: (f64, f64) = (0.70, 0.85); // warning zone
const PENALTY_SCORE: i64 = 15; // cpu penalty
const DEFAULT_SCORE: i64 = 10;

pub async fn prioritize_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExtenderArgs>,
) -> Json<Vec<HostPriority>> {
    info!("=== PRIORITIZATION PHASE STARTED ===");
    debug!("Received payload: {:?}", payload);

    let mut priorities = Vec::new();

    let candidate_nodes: Vec<String> = if !payload.nodes.items.is_empty() {
        payload.nodes.items.into_iter()
            .filter_map(|n| n.metadata.name)
            .collect()
    } else if let Some(names) = payload.node_names.clone() {
        names
    } else {
        state.get_last_filtered_nodes().await
    };

    for node_name in candidate_nodes {
        let score = match state.get_probe(&node_name).await {
            Some(probe) => {
                let mut final_score = probe.to_scheduler_score(CPU_WEIGHT, LATENCY_WEIGHT);

                // CPU penalty applied if the node score is in the warning zone
                if probe.cpu_ewma_score >= PENALTY_CPU_RANGE.0
                    && probe.cpu_ewma_score <= PENALTY_CPU_RANGE.1
                {
                    let before = final_score;
                    final_score = (final_score - PENALTY_SCORE).max(0);
                    trace!("Penalty applied (CPU warning zone): {} → {}", before, final_score);
                }

                final_score
            }
            None => {
                info!(
                    "No probe data for {}, assigning default score {}",
                    node_name, DEFAULT_SCORE
                );
                DEFAULT_SCORE
            }
        };

        priorities.push(HostPriority { host: node_name.clone(), score });
        info!("Node {} assigned score: {}", node_name, score);
    }

    let mut sorted_priorities = priorities.clone();
    sorted_priorities.sort_by(|a, b| b.score.cmp(&a.score));

    info!("Node ranking:");
    for (i, p) in sorted_priorities.iter().enumerate() {
        info!("  {}. {} = {}", i + 1, p.host, p.score);
    }

    if let Some(best) = sorted_priorities.first() {
        state.increment_pod_count(&best.host).await;
        info!("Incremented pod count for {}", best.host);
    }

    info!("=== PRIORITIZATION PHASE COMPLETED ===");
    Json(priorities)
}
