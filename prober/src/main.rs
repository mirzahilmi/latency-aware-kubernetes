use std::{env, time::Duration};

use ::prober::{Prober, prober::Latency};
use prober::new_prober;
use prost::Message;
use rumqttc::{Client, Event, MqttOptions, Packet, QoS, SubscribeFilter};

fn main() {
    let k8s_node = env::var("KUBERNETES_NODE").unwrap_or(String::from("N/A"));
    let mqtt_host = env::var("MQTT_HOST").unwrap_or(String::from("test.mosquitto.org"));
    let mqtt_port = env::var("MQTT_PORT")
        .unwrap_or(String::from("1883"))
        .parse()
        .unwrap();
    let mqtt_client_id = format!("mirzaganteng-{k8s_node}");
    let mqtt_discover_topic =
        env::var("MQTT_RESULT_TOPIC").unwrap_or(String::from("prober/discover"));
    let mqtt_result_topic = env::var("MQTT_RESULT_TOPIC").unwrap_or(String::from("prober/result"));

    let mut mqtt_options = MqttOptions::new(mqtt_client_id, mqtt_host, mqtt_port);
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (client, mut connection) = Client::new(mqtt_options, 10);
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

    println!("Listening for MQTT notification");
    for notification in connection.iter() {
        let event = match notification {
            Ok(event) => event,
            Err(e) => {
                println!("Failed connection: {e}");
                continue;
            }
        };
        let Event::Incoming(inbound) = event else {
            continue;
        };
        let Packet::Publish(packet) = inbound else {
            continue;
        };
        if packet.topic != mqtt_discover_topic {
            continue;
        }
        let target = match String::from_utf8(packet.payload.to_vec()) {
            Ok(target) => target,
            Err(e) => {
                println!("Failed to parse packet payload bytes: {e}");
                continue;
            }
        };
        let prober = new_prober(target.clone());
        let metrics = match prober.ping(5) {
            Ok(metrics) => metrics,
            Err(e) => {
                println!("Failed to parse packet payload bytes: {e}");
                continue;
            }
        };
        let result = Latency {
            ip_destination: target,
            node_source: k8s_node.clone(),
            metrics,
        };
        if let Err(e) = client.publish(
            &mqtt_result_topic,
            QoS::ExactlyOnce,
            false,
            Latency::encode_to_vec(&result),
        ) {
            println!("Failed to publish metric: {e}");
            continue;
        }
    }
}
