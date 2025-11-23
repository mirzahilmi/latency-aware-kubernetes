use kube::runtime;
use std::{collections::HashMap, net::IpAddr};

use futures::TryStreamExt;
use k8s_openapi::api::core::v1::{EndpointSubset, Endpoints, Service as KubernetesService};
use kube::{
    Api, Client, ResourceExt,
    runtime::{WatchStreamExt, watcher::Config},
};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::{
    actor::{Event, Service},
    config::Config as AppConfig,
};

enum Control<E> {
    Watcher(E),
    Stop,
}

pub async fn watch_endpoints(
    config: AppConfig,
    tx: broadcast::Sender<Event>,
) -> anyhow::Result<()> {
    let client = Client::try_default().await?;
    let endpoints_api: Api<Endpoints> = Api::namespaced(client.clone(), &config.namespace);
    let service_api: Api<KubernetesService> = Api::namespaced(client, &config.namespace);

    let result = runtime::watcher(endpoints_api, Config::default())
        .applied_objects()
        .default_backoff()
        .map_err(Control::Watcher)
        .try_for_each(|endpoints| {
            let tx = tx.clone();
            let service_api = service_api.clone();
            async move {
                let servicename = endpoints.name_any();
                info!("actor: endpoints changes occured for {servicename} service");
                let Some(EndpointSubset {
                    addresses: Some(addresses),
                    ..
                }) = endpoints
                    .subsets
                    .as_ref()
                    .and_then(|subsets| subsets.first())
                else {
                    warn!("actor: empty subsets from endpointslice {servicename}");
                    return Ok(());
                };

                let service = match service_api.get(&servicename).await {
                    Ok(service) => service,
                    Err(e) => {
                        error!(
                            "actor: failed to get endpoints service object of {servicename}: {e}"
                        );
                        return Ok(());
                    }
                };
                let Some(serviceip) = service.spec.and_then(|spec| spec.cluster_ip) else {
                    warn!("actor: cannot find the clusterIp for service {servicename}");
                    return Ok(());
                };
                let serviceip = match serviceip.parse::<IpAddr>() {
                    Ok(ip) => ip,
                    Err(e) => {
                        error!("actor: failed to parse string ip: {e}");
                        return Ok(());
                    }
                };

                let mut endpoints_by_nodename = HashMap::<String, Vec<String>>::new();
                for address in addresses {
                    let ip = address.ip.clone();
                    let Some(nodename) = &address.node_name else {
                        warn!("actor: missing nodename for pod endpoint of {ip}");
                        continue;
                    };
                    let mut endpoints = match endpoints_by_nodename.get(nodename) {
                        Some(endpoints) => endpoints.clone(),
                        None => Vec::new(),
                    };
                    endpoints.push(ip);
                    endpoints_by_nodename.insert(nodename.clone(), endpoints);
                }

                info!("actor: captured service {servicename} endpoints changes: {endpoints_by_nodename:?}");
                if endpoints_by_nodename.len() == 1 {
                    info!("actor: skipping undistributed service {servicename} endpoints containing only 1 node");
                    return Ok(());
                }

                let service = Service {
                    name: servicename,
                    ip: serviceip,
                    endpoints_by_nodename,
                };
                if let Err(e) = tx.send(Event::ServiceChanged(service)) {
                    info!("actor: latency probe exiting: {e}");
                    return Err(Control::Stop);
                };

                Ok(())
            }
        })
        .await;

    if let Err(Control::Watcher(e)) = result {
        return Err(e.into());
    }

    Ok(())
}
