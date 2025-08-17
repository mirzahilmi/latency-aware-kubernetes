use anyhow::{Result, anyhow};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Client,
    api::{Api, AttachParams, ListParams},
};
use serde::Deserialize;
use std::collections::HashMap;
use tokio::io::AsyncReadExt;
use tracing::{info, warn};

#[derive(Deserialize, Debug)]
struct CiliumService {
    spec: CiliumServiceSpec,
}

#[derive(Deserialize, Debug)]
struct CiliumServiceSpec {
    id: u32,
    #[serde(rename = "frontend-address")]
    frontend_address: CiliumFrontend,
    #[serde(rename = "backend-addresses")]
    backend_addresses: Option<Vec<CiliumBackend>>,
    flags: CiliumServiceFlags,
}

#[derive(Deserialize, Debug)]
struct CiliumServiceFlags {
    name: String,
}

#[derive(Deserialize, Debug)]
struct CiliumFrontend {
    ip: String,
    port: u32,
}

#[derive(Deserialize, Debug)]
struct CiliumBackend {
    ip: String,
    port: u32,
}

async fn get_cilium_pod_name(client: Client, node_name: &str) -> Result<String> {
    let pods: Api<Pod> = Api::namespaced(client, "kube-system");
    let lp = ListParams::default()
        .labels("k8s-app=cilium")
        .fields(&format!("spec.nodeName={}", node_name));
    let cilium_pods = pods.list(&lp).await?;

    if let Some(pod) = cilium_pods.items.first() {
        if let Some(name) = &pod.metadata.name {
            return Ok(name.clone());
        }
    }

    Err(anyhow!("Cilium pod not found on node {}", node_name))
}

async fn exec_in_pod(
    client: Client,
    pod_name: &str,
    namespace: &str,
    command: Vec<&str>,
) -> Result<String> {
    let pods: Api<Pod> = Api::namespaced(client, namespace);
    let attach_params = AttachParams::default().stderr(false);
    let mut attached = pods.exec(pod_name, command, &attach_params).await?;

    let mut stdout_str = String::new();
    if let Some(mut stdout) = attached.stdout() {
        stdout.read_to_string(&mut stdout_str).await?;
    }

    attached.join().await?;

    Ok(stdout_str)
}

pub async fn update_cilium_service_weights(
    current_node_name: &str,
    service_name: &str,
    latency_caches: &HashMap<String, f64>,
) -> Result<()> {
    info!("Updating Cilium service weights...");
    if latency_caches.is_empty() {
        warn!("Latency cache is empty, skipping update.");
        return Ok(());
    }

    let client = Client::try_default().await?;
    let cilium_pod_name = get_cilium_pod_name(client.clone(), current_node_name).await?;
    info!("Found Cilium pod: {}", cilium_pod_name);

    let command = vec!["cilium", "service", "list", "-o", "json"];
    let output = exec_in_pod(client.clone(), &cilium_pod_name, "kube-system", command).await?;

    let services: Vec<CiliumService> = serde_json::from_str(&output)?;
    let target_services: Vec<CiliumService> = services
        .into_iter()
        .filter(|s| s.spec.flags.name == service_name)
        .collect();

    if target_services.is_empty() {
        warn!("Service {} not found.", service_name);
        return Ok(());
    }

    let min_latency = latency_caches
        .values()
        .filter(|&&v| v > 0.0)
        .cloned()
        .fold(f64::INFINITY, f64::min);

    if min_latency.is_infinite() {
        warn!("No valid latencies in cache, skipping update.");
        return Ok(());
    }

    for service in target_services {
        let Some(backends) = &service.spec.backend_addresses else {
            warn!(
                "Service {} with frontend {}:{} has no backends. Skipping.",
                service_name, service.spec.frontend_address.ip, service.spec.frontend_address.port
            );
            continue;
        };

        let weights: Vec<u32> = backends
            .iter()
            .map(|_| {
                let latency = latency_caches
                    .get(&service.spec.frontend_address.ip.to_string())
                    .cloned()
                    .unwrap_or(0.0);

                if latency > 0.0 {
                    ((min_latency / latency) * 100.0).round().clamp(1.0, 1000.0) as u32
                } else {
                    100 // Default weight
                }
            })
            .collect();

        let backends = backends
            .iter()
            .map(|backend| format!("{}:{}", backend.ip, backend.port))
            .collect::<Vec<String>>()
            .join(",");

        let weights = weights
            .iter()
            .map(|weight| weight.to_string())
            .collect::<Vec<String>>()
            .join(",");

        info!(
            "Updating service id {} with backends {} and weights {}",
            service.spec.id, &backends, &weights
        );

        let frontend = format!(
            "{}:{}",
            service.spec.frontend_address.ip, service.spec.frontend_address.port
        );

        let id = service.spec.id.to_string();

        let update_cmd = vec![
            "cilium",
            "service",
            "update",
            "--id",
            &id,
            "--frontend",
            &frontend,
            "--backends",
            &backends,
            "--backend-weights",
            &weights,
        ];

        if let Err(e) =
            exec_in_pod(client.clone(), &cilium_pod_name, "kube-system", update_cmd).await
        {
            warn!("Failed to update service id {}: {}", service.spec.id, e);
        }
    }

    info!("Cilium service weights updated successfully.");
    Ok(())
}
