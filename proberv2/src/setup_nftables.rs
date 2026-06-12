use anyhow::anyhow;
use k8s_openapi::api::core::v1::Node;
use kube::{Api, Client, ResourceExt};
use nftables::{
    batch::Batch,
    expr::Expression as NftExpression,
    helper,
    schema::{Chain, NfListObject, Rule, Set, SetType, SetTypeValue, Table},
    stmt::{JumpTarget, Statement},
    types::{NfChainPolicy, NfChainType, NfFamily, NfHook},
};
use serde_json::json;
use std::{borrow::Cow, net::IpAddr};
use tracing::{debug, info};

use crate::config::Config;

pub async fn setup_nftables(config: &Config) -> anyhow::Result<()> {
    // inisialisasi klien API Node
    info!("actor: configuring base nftables ruleset");
    let client = Client::try_default().await?;
    let api: Api<Node> = Api::all(client);

    // mengambil IP private yang digunakan oleh worker node saat ini
    let node = api.get(&config.kubernetes.node_name).await?;
    let Some(addresses) = node
        .status
        .as_ref()
        .and_then(|status| status.addresses.as_ref())
    else {
        return Err(anyhow!(
            "missing node {} addresses attribute",
            node.name_any()
        ));
    };
    let Some(a) = addresses
        .iter()
        .find(|address| address.type_ == "InternalIP")
    else {
        return Err(anyhow!(
            "missing node {} InternalIP address",
            node.name_any()
        ));
    };
    let ip = a.address.parse::<IpAddr>()?;

    // menghapus tabel kustom yang telah dibuat jika program telah berjalan sebelumnya
    let mut batch = Batch::new();
    batch.delete(NfListObject::Table(Table {
        name: config.nftables.table.clone().into(),
        family: NfFamily::IP,
        ..Default::default()
    }));
    let ruleset = batch.to_nftables();
    // mengabaikan error menghapus tabel jika tidak ada
    let _ = helper::apply_ruleset(&ruleset);

    // membuat tabel kustom dengan nama spesifik dari konfigurasi yang akan digunakan
    // untuk menampung aturan packet forwarding kustom yang akan diimplementasikan nantinya
    let mut batch = Batch::new();
    batch.add(NfListObject::Table(Table {
        name: config.nftables.table.clone().into(),
        family: NfFamily::IP,
        ..Default::default()
    }));
    let ruleset = batch.to_nftables();
    debug!(
        "actor: applying table: {}",
        serde_json::to_string(&ruleset)?
    );
    helper::apply_ruleset(&ruleset)?;

    // menambahkan struktur data map (service_chain_by_nodeport) yang akan mengandung port dari
    // NodePort sebagai key dan chain packet forwarding spesifik dari Service sebagai value
    helper::apply_ruleset_raw(
        json!({
          "nftables": [
            {
              "add": {
                "map": {
                  "family": "ip",
                  "table": config.nftables.table,
                  "name": config.nftables.map_service_chain_by_nodeport,
                  "type": [
                    "inet_proto",
                    "inet_service"
                  ],
                  "map": "verdict",
                  "comment": "VERDICTS! MUAHAHAHAHA"
                }
              }
            }
          ]
        })
        .to_string()
        .as_ref(),
        None::<&str>,
        std::iter::empty::<&str>(),
    )?;

    // membuat struktur data Set (allowed_node_ips) untuk memastikan packet forwarding
    // hanya dilakukan pada port yang terdaftar sebagai Service NodePort
    let mut batch = Batch::new();
    let ip_sets = [NftExpression::String(ip.to_string().into())];
    batch.add(NfListObject::Set(
        Set {
            family: NfFamily::IP,
            table: config.nftables.table.clone().into(),
            name: config.nftables.set_allowed_node_ips.clone().into(),
            set_type: SetTypeValue::Single(SetType::Ipv4Addr),
            comment: Some("List IPv4 yang nerima traffic dari NodePort".into()),
            elem: Some(ip_sets.as_ref().into()),
            ..Default::default()
        }
        .into(),
    ));
    let ruleset = batch.to_nftables();
    debug!(
        "actor: applying set ruleset: {}",
        serde_json::to_string(&ruleset)?
    );
    helper::apply_ruleset(&ruleset)?;

    let mut batch = Batch::new();
    // membuat chain khusus (prerouting) yang dieksekusi tepat sebelum chain dari kubernetes dijalan
    // dengan menggunakan prioritas yang lebih tinggi
    batch.add(NfListObject::Chain(Chain {
        family: NfFamily::IP,
        table: config.nftables.table.clone().into(),
        name: config.nftables.chain_prerouting.clone().into(),
        _type: NfChainType::NAT.into(),
        hook: NfHook::Prerouting.into(),
        prio: (-150).into(),
        policy: NfChainPolicy::Accept.into(),
        ..Default::default()
    }));
    // membuat chain (services) untuk:
    // 1. memastikan IP destinasi paket berada pada set
    // 2. mengarahkan paket pada aturan chain yang sesuai berdasarkan port yang dituju
    //    menggunakan lookup O(1) melalui map (service_by_nodeport)
    batch.add(NfListObject::Chain(Chain {
        family: NfFamily::IP,
        table: config.nftables.table.clone().into(),
        name: config.nftables.chain_services.clone().into(),
        ..Default::default()
    }));
    let ruleset = batch.to_nftables();
    debug!(
        "actor: applying chain: {}",
        serde_json::to_string(&ruleset)?
    );
    helper::apply_ruleset(&ruleset)?;

    // menambahkan aturan pada chain (prerouting) untuk
    // melanjutkan pemrosesan paket kepada chain (services)
    let mut batch = Batch::new();
    batch.add(NfListObject::Rule(Rule {
        family: NfFamily::IP,
        table: config.nftables.table.clone().into(),
        chain: config.nftables.chain_prerouting.clone().into(),
        expr: Cow::Owned(vec![Statement::Jump(JumpTarget {
            target: config.nftables.chain_services.clone().into(),
        })]),
        ..Default::default()
    }));
    let ruleset = batch.to_nftables();
    debug!(
        "actor: applying chain: {}",
        serde_json::to_string(&ruleset)?
    );
    helper::apply_ruleset(&ruleset)?;

    // menambahkan aturan pada chain (services) untuk melakukan
    // lookup chain yang dituju oleh paket berdasarkan destinasi port
    let rule = json!(
    {
      "nftables": [
        {
          "add": {
            "rule": {
              "family": "ip",
              "table": config.nftables.table,
              "chain": config.nftables.chain_services,
              "comment": "Cek IPv4 paket di list IPv4 NodePort, kalo ada langsung ke verdict map ke service yang sesuai",
              "expr": [
                {
                  "match": {
                    "op": "==",
                    "left": {
                      "payload": {
                        "protocol": "ip",
                        "field": "daddr"
                      }
                    },
                    "right": format!("@{}", config.nftables.set_allowed_node_ips)
                  }
                },
                {
                  "vmap": {
                    "key": {
                      "concat": [
                        {
                          "meta": {
                            "key": "l4proto"
                          }
                        },
                        {
                          "payload": {
                            "protocol": "th",
                            "field": "dport"
                          }
                        }
                      ]
                    },
                    "data": format!("@{}", config.nftables.map_service_chain_by_nodeport)
                  }
                }
              ]
            }
          }
        }
      ]
    }
    );

    debug!("actor: applying initial ruleset: {}", rule.to_string());
    helper::apply_ruleset_raw(&rule.to_string(), None::<&str>, std::iter::empty::<&str>())?;

    Ok(())
}
