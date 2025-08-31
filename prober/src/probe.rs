use anyhow::Result;
use k8s_openapi::{
    api::core::v1::Node,
    serde::{Deserialize, Serialize},
};
use kube::{Api, Client, api::ListParams, core::Expression};
use std::{collections::HashMap, time::Duration};
use tokio::time;
use tracing::{debug, error, info};

use crate::ping;

pub struct Prober<'a> {
    client: Client,
    caches: &'a mut HashMap<String, f64>,
}

impl<'a> Prober<'a> {
    pub fn new(caches: &'a mut HashMap<String, f64>, client: Client) -> Self {
        Self { caches, client }
    }

    pub async fn probe(&mut self, retry_threshold: u32, ping_count: u32) {
        let mut attempts = 0;
        while attempts < retry_threshold {
            if self.try_probe(ping_count).await.is_ok() {
                info!("Probing successful");
                return;
            }
            attempts += 1;
            error!(
                "Probe attempt {}/{} failed. Retrying...",
                attempts, retry_threshold
            );
            time::sleep(Duration::from_secs(1)).await;
        }

        error!(
            "All {} probe attempts failed. Continuing to next cycle.",
            retry_threshold
        );
    }

    async fn try_probe(&mut self, ping_count: u32) -> Result<()> {
        info!("Attempting to probe...");

        let nodes_api = Api::<Node>::all(self.client.clone());
        let exp =
            Expression::DoesNotExist("node-role.kubernetes.io/control-plane".to_string()).into();
        let matcher = ListParams::default().labels_from(&exp);
        let nodes = nodes_api.list(&matcher).await?;

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
            self.caches.insert(target, latency);
        }
        debug!("Caches updated: {:?}", self.caches);

        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub host: String,
    pub score: u64,
}
