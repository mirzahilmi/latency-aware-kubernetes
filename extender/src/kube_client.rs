use kube::{api::{Api, ListParams}, Client};
use k8s_openapi::api::core::v1::Pod;
use std::collections::HashMap;

/// Return mapping: nodeName -> "<podname>.<base>:<port>"
pub async fn get_prober_mapping(namespace: &str) -> anyhow::Result<HashMap<String, String>> {
    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::namespaced(client, namespace);

    // NOTE: samakan label ini dengan StatefulSet Prober-mu (app=prober)
    let lp = ListParams::default().labels("app=prober");
    let list = pods.list(&lp).await?;

    // Ambil BASE & PORT dari env
    let base = std::env::var("PROBER_BASE")
        .unwrap_or_else(|_| format!("prober.{}.svc.cluster.local", namespace));
    let port = std::env::var("PROBER_PORT").unwrap_or_else(|_| "3000".to_string());

    let mut map = HashMap::new();
    for p in list {
        let pod_name = match &p.metadata.name {
            Some(n) => n.clone(),
            None => continue,
        };
        let node_name = match &p.spec.as_ref().and_then(|s| s.node_name.clone()) {
            Some(n) => n.clone(),
            None => continue,
        };
        
        let host = format!("{}.{}:{}", pod_name, base, port);
        map.insert(node_name, host);
    }
    Ok(map)
}
