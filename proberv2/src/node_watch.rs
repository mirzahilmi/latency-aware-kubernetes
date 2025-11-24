use crate::actor::{Event, WorkerNode};
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::Node;
use kube::{
    Api, Client, ResourceExt,
    api::ListParams,
    core::Expression,
    runtime::{self, WatchStreamExt, watcher::Config},
};
use std::net::IpAddr;
use tokio::sync::broadcast;
use tracing::{info, warn};

enum Control<E> {
    Watcher(E),
    Stop,
}

pub async fn watch_nodes(tx: broadcast::Sender<Event>) -> anyhow::Result<()> {
    let client = Client::try_default().await?;
    let api: Api<Node> = Api::all(client);
    let matcher = ListParams::default().labels_from(
        &Expression::DoesNotExist("node-role.kubernetes.io/control-plane".to_string()).into(),
    );

    // initial discovery
    let nodes = api.list(&matcher).await?;
    for node in nodes {
        let Some(addrs) = node
            .status
            .as_ref()
            .and_then(|status| status.addresses.as_ref())
        else {
            continue;
        };
        let Some(a) = addrs.iter().find(|x| x.type_ == "InternalIP") else {
            continue;
        };

        let Ok(ip) = a.address.parse::<IpAddr>() else {
            warn!("actor: invalid ip {}", a.address);
            continue;
        };

        tx.send(Event::NodeJoined(WorkerNode {
            name: node.name_any(),
            ip,
        }))
        .ok();
    }

    // watch loop
    let result = runtime::watcher(api, Config::default())
        .applied_objects()
        .default_backoff()
        .map_err(Control::Watcher)
        .try_for_each(|node| {
            let tx = tx.clone();
            async move {
                let Some(status) = &node.status else {
                    return Ok(());
                };
                let Some(addrs) = &status.addresses else {
                    return Ok(());
                };
                let Some(a) = addrs.iter().find(|x| x.type_ == "InternalIP") else {
                    return Ok(());
                };

                let Ok(ip) = a.address.parse::<IpAddr>() else {
                    warn!("actor: invalid ip {}", a.address);
                    return Ok(());
                };

                if let Err(e) = tx.send(Event::NodeJoined(WorkerNode {
                    name: node.name_any(),
                    ip,
                })) {
                    info!("actor: stopping node watcher: {e}");
                    return Err(Control::Stop);
                }

                Ok(())
            }
        })
        .await;

    if let Err(Control::Watcher(e)) = result {
        return Err(e.into());
    }

    Ok(())
}
