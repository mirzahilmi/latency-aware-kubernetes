use anyhow::Result;
use std::{collections::HashMap, time::Duration};
use tokio::time;
use tracing::{debug, error, info};

use k8s_openapi::api::core::v1::Node;
use kube::{Api, Client, api::ListParams, core::Expression};

use crate::ping;

pub async fn probe(retry_threshold: u32, ping_count: u32, caches: &mut HashMap<String, f64>) {
    let mut attempts = 0;
    while attempts < retry_threshold {
        match try_probe(caches, ping_count).await {
            Ok(_) => {
                info!("Probing successful");
                return;
            }
            Err(e) => {
                attempts += 1;
                error!(
                    "Probe attempt {}/{} failed: {}. Retrying...",
                    attempts, retry_threshold, e
                );
                time::sleep(Duration::from_secs(1)).await;
            }
        }
    }

    error!(
        "All {} probe attempts failed. Continuing to next cycle.",
        retry_threshold
    );
}

async fn try_probe(caches: &mut HashMap<String, f64>, ping_count: u32) -> Result<()> {
    info!("Program starting...");

    let client = Client::try_default().await?;
    let nodes = Api::<Node>::all(client.clone());

    let exp = Expression::DoesNotExist("node-role.kubernetes.io/control-plane".to_string()).into();
    let matcher = ListParams::default().labels_from(&exp);
    let nodes = nodes.list(&matcher).await?;

    let targets: Vec<String> = nodes
        .iter()
        .filter_map(|node| {
            node.status.as_ref().and_then(|status| {
                status.addresses.as_ref().and_then(|addresses| {
                    addresses
                        .iter()
                        .find(|address| address.type_ == "InternalIP")
                        .map(|address| address.address.clone())
                })
            })
        })
        .collect();
    debug!("Receive probing targets {:?}", targets);

    let results = ping::ping(targets, ping_count).await?;
    for (target, latency) in results.into_iter() {
        caches.insert(target, latency);
    }
    debug!("Caches updated: {:?}", caches);

    Ok(())
}
