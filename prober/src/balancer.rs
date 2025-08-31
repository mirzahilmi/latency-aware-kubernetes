use std::collections::HashMap;

use futures::TryStreamExt;
use k8s_openapi::api::core::v1::Endpoints;
use kube::{
    Api, Client,
    runtime::{self, WatchStreamExt, watcher},
};
use nftables::{helper, schema::NfObject};

pub struct Balancer<'a> {
    client: Client,
    caches: &'a HashMap<String, f64>,
}

impl<'a> Balancer<'a> {
    pub fn new(client: Client, caches: &'a HashMap<String, f64>) -> Self {
        Balancer { client, caches }
    }

    pub async fn watch(&self, service_name: &str) -> anyhow::Result<()> {
        // surely better of putting namespace outside
        let endpoints: Api<Endpoints> = Api::namespaced(self.client.clone(), "riset");
        runtime::watcher(
            endpoints,
            watcher::Config::default().fields(format!("metadata.name={service_name}").as_str()),
        )
        .applied_objects()
        .default_backoff()
        .try_for_each(|event| async move {
            // TODO: propagate the error
            let _ = self.reconcile(event).await;
            Ok(())
        })
        .await?;
        Ok(())
    }

    async fn reconcile(&self, endpoints: Endpoints) -> anyhow::Result<()> {
        let rulesets = helper::get_current_ruleset()?.objects.into_owned();
        for ruleset in rulesets {
            let NfObject::ListObject(ruleset) = ruleset else {
                continue;
            };
        }
        Ok(())
    }
}
