use std::{
    env,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use ::prober::Prober;
use prober::{Ping, prober::LatencyAggr};
use prost::Message;
use rumqttc::{Client, Event, MqttOptions, Outgoing, QoS, SubscribeFilter};

fn main() {
    let targets: Vec<String> = env::args().skip(1).collect();
    if targets.is_empty() {
        panic!("Targets cannot be empty");
    }

    targets.iter().for_each(|target| println!("here {target}"));

    let k8s_node = env::var("KUBERNETES_NODE").unwrap_or(String::from("N/A"));
    let mqtt_host = env::var("MQTT_HOST").unwrap_or(String::from("localhost"));
    let mqtt_port = env::var("MQTT_PORT")
        .unwrap_or(String::from("1883"))
        .parse()
        .unwrap();
    let mqtt_client_id = format!("mirzaganteng-{k8s_node}");
    let mqtt_discover_topic =
        env::var("MQTT_DISCOVER_TOPIC").unwrap_or(String::from("prober/discover"));
    let mqtt_result_topic = env::var("MQTT_RESULT_TOPIC").unwrap_or(String::from("prober/result"));

    let mut mqtt_options = MqttOptions::new(mqtt_client_id, mqtt_host, mqtt_port);
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (client, mut conn) = Client::new(mqtt_options, 10);
    client
        .subscribe_many([
            SubscribeFilter {
                path: mqtt_discover_topic.clone(),
                qos: QoS::AtLeastOnce,
            },
            SubscribeFilter {
                path: mqtt_result_topic.clone(),
                qos: QoS::AtLeastOnce,
            },
        ])
        .unwrap();

    let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();

    // pool mqtt connection in separate thread
    thread::spawn(move || {
        for notification in conn.iter() {
            let event = notification.unwrap();
            let Event::Outgoing(packet) = event else {
                continue;
            };
            if let Outgoing::Publish(_) = packet {
                // send signal when publish packet received
                // and stop connection pooling
                tx.send(()).unwrap();
                break;
            }
        }
    });

    let prober = Ping { targets };
    let latencies = prober.ping(5).unwrap();
    latencies[0]
        .metrics
        .iter()
        .for_each(|metric| println!("DEBUG: {metric}"));
    let aggr = LatencyAggr {
        node_source: k8s_node,
        latencies,
    };

    if let Err(e) = client.publish(
        &mqtt_result_topic,
        QoS::ExactlyOnce,
        false,
        aggr.encode_to_vec(),
    ) {
        println!("Failed to publish metric: {e}");
    }
    println!("Published metrics");

    // block until signal received
    rx.recv().unwrap();
}
