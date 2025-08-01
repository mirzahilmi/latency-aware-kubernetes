use std::{env, time::Duration};
use tracing::{debug, info};

use ::prober::Prober;
use k8s_openapi::api::core::v1::Node;
use kube::{Api, Client, Resource, api::ListParams, core::Expression};
use prober::{Ping, prober::LatencyAggr};
use prost::Message;
use rumqttc::{AsyncClient, MqttOptions, QoS};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let delay = env::var("INTERVAL_IN_SECONDS")?.parse()?;
    loop {
        probe().await?;
        tokio::time::sleep(Duration::from_secs(delay)).await;
    }
}

async fn probe() -> Result<(), Box<dyn std::error::Error>> {
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

    tokio::spawn(async move {
        info!("MQTT connection pooling started");
        while conn.poll().await.is_ok() {}
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

    Ok(())
}
