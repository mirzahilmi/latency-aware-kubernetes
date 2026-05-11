use kube::runtime;
use std::{collections::HashMap, net::Ipv4Addr};
use tokio_util::sync::CancellationToken;

use futures::TryStreamExt;
use k8s_openapi::{
    api::core::v1::{EndpointSubset, Endpoints, Service as KubernetesService},
    apimachinery::pkg::util::intstr::IntOrString,
};
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
    token: CancellationToken,
) -> anyhow::Result<()> {
    let client = Client::try_default().await?;
    let endpoints_api: Api<Endpoints> =
        Api::namespaced(client.clone(), &config.kubernetes.namespace);
    let service_api: Api<KubernetesService> = Api::namespaced(client, &config.kubernetes.namespace);

    // berlangganan perubahan endpoint aplikasi untuk menyesuaikan ip pod secara real-time
    let handler = runtime::watcher(endpoints_api, Config::default().fields(&format!("metadata.name={}", config.kubernetes.service)))
        .applied_objects()
        .default_backoff()
        .map_err(Control::Watcher)
        .try_for_each(|endpoints| {
            let tx = tx.clone();
            let service_api = service_api.clone();
            async move {
                // mengambil nama Service dari Endpoints
                let servicename = endpoints.name_any();
                info!("actor: endpoints changes occured for {servicename} service");

                // mengambil property addresses dari Endpoints yang merupakan 
                // sekumpulan alamat IP dari pod aplikasi
                let Some(EndpointSubset { addresses: Some(addresses), .. }) = endpoints
                    .subsets
                    .as_ref()
                    .and_then(|subsets| subsets.first())
                else {
                    warn!("actor: empty subsets from endpointslice {servicename}");
                    return Ok(());
                };

                // melakukan query untuk mendapatkan Service
                // berdasarkan nama Service yang diperoleh dari Endpoints
                let service = match service_api.get(&servicename).await {
                    Ok(service) => service,
                    Err(e) => {
                        error!("actor: failed to get endpoints service object of {servicename}: {e}");
                        return Ok(());
                    }
                };

                // mengambil port pertama dari property ports yang terdafar pada Service
                let Some(port) = service.spec.as_ref()
                    .and_then(|spec| spec.ports.as_ref())
                    .and_then(|ports| ports.first()) else {
                        warn!("actor: cannot find any ports for service {servicename}");
                        return Ok(());
                    };

                // mengambil port NodePort
                let Some(nodeport) = port.node_port else {
                    warn!("actor: cannot find any ports for service {servicename}");
                    return Ok(());
                };

                // mengambil target port yang dituju dari port NodePort
                let targetport = match port.target_port {
                    Some(IntOrString::Int(port)) => port,
                    _ => port.port,
                };

                // inisialisasi map untuk pemetaan/grouping endpoints berdasarkan node
                let mut endpoints_by_nodename = HashMap::<String, Vec<Ipv4Addr>>::new();

                // melakukan pemetaan/grouping endpoints berdasarkan node
                for address in addresses {
                    let ip = match address.ip.clone().parse::<Ipv4Addr>() {
                        Ok(ip) => ip,
                        Err(e) => {
                            error!("actor: invalid ipv4 string: {e}");
                            continue;
                        }
                    };
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

                // lewati jika Endpoints hanya terdaftar pada 1 node
                if endpoints_by_nodename.len() == 1 {
                    info!("actor: skipping undistributed service {servicename} endpoints containing only 1 node");
                    return Ok(());
                }

                // mengirim informasi penuh terkait sebuah Service (nama, NodePort, port target, kelompok endpoints berdasarkan node)
                // sebagai event ServiceChanged melalui channel untuk dikonsumsi proses lain
                let service = Service { name: servicename, nodeport, targetport, endpoints_by_nodename };
                if let Err(e) = tx.send(Event::ServiceChanged(service)) {
                    info!("actor: latency probe exiting: {e}");
                    // memberhentikan langganan ketika gagal mengirim event NodeJoined pada channel
                    // yang berarti channel telah ditutup karena dalam proses program shutdown
                    return Err(Control::Stop);
                };

                Ok(())
            }
        });

    // menunggu sinyal secara blocking diantara sinyal program shutdown atau
    // langganan perubahan node berhenti untuk memberhentikan fungsi
    tokio::select! {
        _ = token.cancelled() => {
            info!("actor: exiting endpoints_watch task");
            Ok(())
        },
        result = handler => {
            if let Err(Control::Watcher(e)) = result {
                return Err(e.into());
            }
            Ok(())
        }
    }
}
