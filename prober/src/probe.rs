use anyhow::Result;
use futures::lock::Mutex;
use k8s_openapi::{
    api::core::v1::Node,
    serde::{Deserialize, Serialize},
};
use kube::{Api, Client, ResourceExt, api::ListParams, core::Expression};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};
use tokio::time;
use tracing::{debug, error, info};

use crate::ping;

pub struct Prober {
    client: Client,
    caches: Arc<Mutex<HashMap<String, f64>>>,
    sleep_time: Duration,
}

impl Prober {
    pub fn new(
        client: Client,
        caches: Arc<Mutex<HashMap<String, f64>>>,
        sleep_time: Duration,
    ) -> Self {
        Self {
            client,
            caches,
            sleep_time,
        }
    }

    pub async fn watch_probe(&mut self, retry_threshold: u32, ping_count: u32) {
        loop {
            time::sleep(self.sleep_time).await;
            self.probe(retry_threshold, ping_count).await;
        }
    }

    async fn probe(&mut self, retry_threshold: u32, ping_count: u32) {
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

        let targets_map: BTreeMap<_, _> = nodes
            .iter()
            .filter_map(|node| {
                node.status.as_ref().and_then(|status| {
                    status.addresses.as_ref().and_then(|addresses| {
                        addresses
                            .iter()
                            .find(|address| address.type_ == "InternalIP")
                            .map(|address| (address.address.clone(), node.name_any()))
                    })
                })
            })
            .collect();
        debug!("Receive probing targets {:?}", targets_map.keys());

        let results = ping::ping(targets_map.keys().cloned().collect(), ping_count).await?;
        for (target, latency) in results.into_iter() {
            let Some(hostname) = targets_map.get(&target) else {
                continue;
            };
            {
                let mut caches = self.caches.lock().await;
                caches.insert(hostname.to_string(), latency);
            }
        }
        debug!("Caches updated: {:?}", self.caches);

        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProbeTarget {
    pub hostname: String,
    pub ip: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub host: String,
    pub score: f64,
}
