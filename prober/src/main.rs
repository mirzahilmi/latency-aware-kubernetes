use std::{env, time::Duration};
use tracing::{debug, info};

use ::prober::Prober;
use k8s_openapi::api::core::v1::Node;
use kube::{Api, Client, Resource, api::ListParams, core::Expression};
use prober::{Ping, prober::LatencyAggr};
use prost::Message;
use rumqttc::{AsyncClient, Event, MqttOptions, Outgoing, QoS};
use tokio::sync::oneshot;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Program starting...");
    let client = Client::try_default().await?;
    let nodes = Api::<Node>::all(client.clone());

    let exp = Expression::DoesNotExist("node-role.kubernetes.io/control-plane".to_string()).into();
    let matcher = ListParams::default().labels_from(&exp);
    let nodes = nodes.list_metadata(&matcher).await?;

    let mut targets = Vec::with_capacity(nodes.iter().count());
    for node in nodes.iter() {
        let Some(name) = node.meta().name.clone() else {
            continue;
        };
        targets.push(name);
    }
    debug!("Receive probing targets {:?}", targets);

    let k8s_node = env::var("KUBERNETES_NODE")?;
    let mqtt_host = env::var("MQTT_HOST")?;
    let mqtt_port = env::var("MQTT_PORT")?.parse()?;
    let mqtt_result_topic = env::var("MQTT_RESULT_TOPIC")?;
    let mqtt_client_id = format!("mirzaganteng-{k8s_node}");

    let mut mqtt_options = MqttOptions::new(mqtt_client_id, mqtt_host, mqtt_port);
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (client, mut conn) = AsyncClient::new(mqtt_options, 10);
    client
        .subscribe(&mqtt_result_topic, QoS::AtLeastOnce)
        .await?;
    info!("Connected to MQTT broker");

    let (tx, rx) = oneshot::channel();

    // pool mqtt connection in separate thread
    tokio::spawn(async move {
        info!("MQTT connection pooling started");
        while let Ok(notification) = conn.poll().await {
            let Event::Outgoing(packet) = notification else {
                continue;
            };
            if let Outgoing::Publish(_) = packet {
                break;
            }
        }
        // send signal when publish packet received
        // and stop connection pooling
        tx.send(()).unwrap();
    });

    let prober = Ping { targets };

    info!("Probe starting...");
    let latencies = prober.ping(5)?;
    let aggr = LatencyAggr {
        node_source: k8s_node,
        latencies,
    };
    debug!("Received probe result {:?}", &aggr.latencies);

    client
        .publish(
            &mqtt_result_topic,
            QoS::ExactlyOnce,
            false,
            aggr.encode_to_vec(),
        )
        .await?;
    info!("Probe result published into MQTT broker");

    // block until signal received
    rx.await?;

    Ok(())
}
