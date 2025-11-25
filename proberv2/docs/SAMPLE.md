## Kubeproxy Service Nftables
```txt
table ip kube-proxy {
        comment "rules for kube-proxy"

        chain nat-prerouting {
                type nat hook prerouting priority dstnat; policy accept;
                jump services
        }

        chain services {
                ip daddr . meta l4proto . th dport vmap @service-ips
                ip daddr @nodeport-ips meta l4proto . th dport vmap @service-nodeports
        }

        map service-ips {
                type ipv4_addr . inet_proto . inet_service : verdict
                comment "ClusterIP, ExternalIP and LoadBalancer IP traffic"
                elements = { 10.43.0.10 . tcp . 53 : goto service-NWBZK7IH-kube-system/kube-dns/tcp/dns-tcp,
                             10.43.0.10 . udp . 53 : goto service-FY5PMXPG-kube-system/kube-dns/udp/dns,
                             10.43.195.72 . tcp . 8000 : goto service-P6J77WGM-riset/hellopod-np-svc/tcp/,
                             10.43.34.64 . tcp . 3001 : goto service-LNJAC4UY-riset/latency-extender-svc/tcp/http,
                             10.43.0.1 . tcp . 443 : goto service-2QRHZV4L-default/kubernetes/tcp/https,
                             10.43.10.165 . tcp . 443 : goto service-WXVQUIFM-kube-system/metrics-server/tcp/https,
                             10.43.0.10 . tcp . 9153 : goto service-AS2KJYAD-kube-system/kube-dns/tcp/metrics,
                             10.43.22.243 . tcp . 4317 : goto service-MBV4MU3G-riset/opentelemetry-collector/tcp/otlp,
                             10.43.22.243 . tcp . 4318 : goto service-Y5V7Q7LN-riset/opentelemetry-collector/tcp/otlp-http }
        }

        map service-nodeports {
                type inet_proto . inet_service : verdict
                comment "NodePort traffic"
                elements = { tcp . 30000 : goto external-P6J77WGM-riset/hellopod-np-svc/tcp/ }
        }

        set nodeport-ips {
                type ipv4_addr
                comment "IPs that accept NodePort traffic"
                elements = { 10.34.4.246 }
        }

        chain external-P6J77WGM-riset/hellopod-np-svc/tcp/ {
                jump mark-for-masquerade
                goto service-P6J77WGM-riset/hellopod-np-svc/tcp/
        }

        chain service-P6J77WGM-riset/hellopod-np-svc/tcp/ {
                ip daddr 10.43.195.72 tcp dport 8000 ip saddr != 10.42.0.0/16 jump mark-for-masquerade
                numgen random mod 9 vmap { 0 : goto endpoint-VNXJG24O-riset/hellopod-np-svc/tcp/__10.42.2.218/8080, 1 : goto endpoint-VGSZZXMZ-riset/hellopod-np-svc/tcp/__10.42.4.201/8080, 2 : goto endpoint-SRWOFVSR-riset/hellopod-np-svc/tcp/__10.42.4.202/8080, 3 : goto endpoint-IMJMHTIB-riset/hellopod-np-svc/tcp/__10.42.4.203/8080, 4 : goto endpoint-YDP3M3R6-riset/hellopod-np-svc/tcp/__10.42.4.204/8080, 5 : goto endpoint-5DYFM3QU-riset/hellopod-np-svc/tcp/__10.42.4.205/8080, 6 : goto endpoint-OK2FDFE2-riset/hellopod-np-svc/tcp/__10.42.4.206/8080, 7 : goto endpoint-JPJY2YFE-riset/hellopod-np-svc/tcp/__10.42.4.207/8080, 8 : goto endpoint-ZWSL4UYY-riset/hellopod-np-svc/tcp/__10.42.5.164/8080 }
        }

        set cluster-ips {
                type ipv4_addr
                comment "Active ClusterIPs"
                elements = { 10.43.0.1, 10.43.0.10,
                             10.43.10.165, 10.43.21.102,
                             10.43.22.243, 10.43.34.64,
                             10.43.195.72 }
        }

        map no-endpoint-services {
                type ipv4_addr . inet_proto . inet_service : verdict
                comment "vmap to drop or reject packets to services with no endpoints"
                elements = { 10.43.21.102 . tcp . 4317 comment "riset/otelcol-opentelemetry-collector:otlp" : goto reject-chain,
                             10.43.21.102 . tcp . 4318 comment "riset/otelcol-opentelemetry-collector:otlp-http" : goto reject-chain }
        }

        map no-endpoint-nodeports {
                type inet_proto . inet_service : verdict
                comment "vmap to drop or reject packets to service nodeports with no endpoints"
        }

        map firewall-ips {
                type ipv4_addr . inet_proto . inet_service : verdict
                comment "destinations that are subject to LoadBalancerSourceRanges"
        }

        chain filter-prerouting {
                type filter hook prerouting priority dstnat - 10; policy accept;
                ct state new jump firewall-check
        }

        chain filter-input {
                type filter hook input priority -110; policy accept;
                ct state new jump nodeport-endpoints-check
                ct state new jump service-endpoints-check
        }

        chain filter-forward {
                type filter hook forward priority -110; policy accept;
                ct state new jump service-endpoints-check
                ct state new jump cluster-ips-check
        }

        chain filter-output {
                type filter hook output priority -110; policy accept;
                ct state new jump service-endpoints-check
                ct state new jump firewall-check
        }

        chain filter-output-post-dnat {
                type filter hook output priority -90; policy accept;
                ct state new jump cluster-ips-check
        }

        chain nat-output {
                type nat hook output priority -100; policy accept;
                jump services
        }

        chain nat-postrouting {
                type nat hook postrouting priority srcnat; policy accept;
                jump masquerading
        }

        chain nodeport-endpoints-check {
                ip daddr @nodeport-ips meta l4proto . th dport vmap @no-endpoint-nodeports
        }

        chain service-endpoints-check {
                ip daddr . meta l4proto . th dport vmap @no-endpoint-services
        }

        chain firewall-check {
                ip daddr . meta l4proto . th dport vmap @firewall-ips
        }

        chain masquerading {
                meta mark & 0x00004000 == 0x00000000 return
                meta mark set meta mark ^ 0x00004000
                masquerade fully-random
        }

        chain cluster-ips-check {
                ip daddr @cluster-ips reject comment "Reject traffic to invalid ports of ClusterIPs"
                ip daddr 10.43.0.0/16 drop comment "Drop traffic to unallocated ClusterIPs"
        }

        chain mark-for-masquerade {
                meta mark set meta mark | 0x00004000
        }

        chain reject-chain {
                comment "helper for @no-endpoint-services / @no-endpoint-nodeports"
                reject
        }

        chain endpoint-VGSZZXMZ-riset/hellopod-np-svc/tcp/__10.42.4.201/8080 {
                ip saddr 10.42.4.201 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.4.201:8080
        }

        chain endpoint-SRWOFVSR-riset/hellopod-np-svc/tcp/__10.42.4.202/8080 {
                ip saddr 10.42.4.202 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.4.202:8080
        }

        chain endpoint-IMJMHTIB-riset/hellopod-np-svc/tcp/__10.42.4.203/8080 {
                ip saddr 10.42.4.203 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.4.203:8080
        }

        chain endpoint-YDP3M3R6-riset/hellopod-np-svc/tcp/__10.42.4.204/8080 {
                ip saddr 10.42.4.204 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.4.204:8080
        }

        chain endpoint-5DYFM3QU-riset/hellopod-np-svc/tcp/__10.42.4.205/8080 {
                ip saddr 10.42.4.205 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.4.205:8080
        }

        chain endpoint-OK2FDFE2-riset/hellopod-np-svc/tcp/__10.42.4.206/8080 {
                ip saddr 10.42.4.206 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.4.206:8080
        }

        chain endpoint-JPJY2YFE-riset/hellopod-np-svc/tcp/__10.42.4.207/8080 {
                ip saddr 10.42.4.207 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.4.207:8080
        }

        chain endpoint-OLBBPMRA-riset/latency-extender-svc/tcp/http__10.42.0.174/3001 {
                ip saddr 10.42.0.174 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.0.174:3001
        }

        chain service-LNJAC4UY-riset/latency-extender-svc/tcp/http {
                ip daddr 10.43.34.64 tcp dport 3001 ip saddr != 10.42.0.0/16 jump mark-for-masquerade
                numgen random mod 1 vmap { 0 : goto endpoint-OLBBPMRA-riset/latency-extender-svc/tcp/http__10.42.0.174/3001 }
        }

        chain endpoint-MIBO4EMV-default/kubernetes/tcp/https__10.34.4.142/6443 {
                ip saddr 10.34.4.142 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.34.4.142:6443
        }

        chain service-2QRHZV4L-default/kubernetes/tcp/https {
                ip daddr 10.43.0.1 tcp dport 443 ip saddr != 10.42.0.0/16 jump mark-for-masquerade
                numgen random mod 1 vmap { 0 : goto endpoint-MIBO4EMV-default/kubernetes/tcp/https__10.34.4.142/6443 }
        }

        chain endpoint-T5HMVMWK-kube-system/kube-dns/tcp/dns-tcp__10.42.0.171/53 {
                ip saddr 10.42.0.171 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.0.171:53
        }

        chain service-NWBZK7IH-kube-system/kube-dns/tcp/dns-tcp {
                ip daddr 10.43.0.10 tcp dport 53 ip saddr != 10.42.0.0/16 jump mark-for-masquerade
                numgen random mod 1 vmap { 0 : goto endpoint-T5HMVMWK-kube-system/kube-dns/tcp/dns-tcp__10.42.0.171/53 }
        }

        chain endpoint-7ZQ7MBHX-kube-system/kube-dns/tcp/metrics__10.42.0.171/9153 {
                ip saddr 10.42.0.171 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.0.171:9153
        }

        chain service-AS2KJYAD-kube-system/kube-dns/tcp/metrics {
                ip daddr 10.43.0.10 tcp dport 9153 ip saddr != 10.42.0.0/16 jump mark-for-masquerade
                numgen random mod 1 vmap { 0 : goto endpoint-7ZQ7MBHX-kube-system/kube-dns/tcp/metrics__10.42.0.171/9153 }
        }

        chain service-MBV4MU3G-riset/opentelemetry-collector/tcp/otlp {
                ip daddr 10.43.22.243 tcp dport 4317 ip saddr != 10.42.0.0/16 jump mark-for-masquerade
                numgen random mod 1 vmap { 0 : goto endpoint-LLU62WPY-riset/opentelemetry-collector/tcp/otlp__10.42.0.181/4317 }
        }

        chain service-Y5V7Q7LN-riset/opentelemetry-collector/tcp/otlp-http {
                ip daddr 10.43.22.243 tcp dport 4318 ip saddr != 10.42.0.0/16 jump mark-for-masquerade
                numgen random mod 1 vmap { 0 : goto endpoint-URN53KYD-riset/opentelemetry-collector/tcp/otlp-http__10.42.0.181/4318 }
        }

        chain endpoint-S4FSIJI2-kube-system/kube-dns/udp/dns__10.42.0.171/53 {
                ip saddr 10.42.0.171 jump mark-for-masquerade
                meta l4proto udp dnat to 10.42.0.171:53
        }

        chain service-FY5PMXPG-kube-system/kube-dns/udp/dns {
                ip daddr 10.43.0.10 udp dport 53 ip saddr != 10.42.0.0/16 jump mark-for-masquerade
                numgen random mod 1 vmap { 0 : goto endpoint-S4FSIJI2-kube-system/kube-dns/udp/dns__10.42.0.171/53 }
        }

        chain endpoint-WGYGAFOZ-kube-system/metrics-server/tcp/https__10.42.0.170/10250 {
                ip saddr 10.42.0.170 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.0.170:10250
        }

        chain service-WXVQUIFM-kube-system/metrics-server/tcp/https {
                ip daddr 10.43.10.165 tcp dport 443 ip saddr != 10.42.0.0/16 jump mark-for-masquerade
                numgen random mod 1 vmap { 0 : goto endpoint-WGYGAFOZ-kube-system/metrics-server/tcp/https__10.42.0.170/10250 }
        }

        chain endpoint-VNXJG24O-riset/hellopod-np-svc/tcp/__10.42.2.218/8080 {
                ip saddr 10.42.2.218 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.2.218:8080
        }

        chain endpoint-ZWSL4UYY-riset/hellopod-np-svc/tcp/__10.42.5.164/8080 {
                ip saddr 10.42.5.164 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.5.164:8080
        }

        chain endpoint-LLU62WPY-riset/opentelemetry-collector/tcp/otlp__10.42.0.181/4317 {
                ip saddr 10.42.0.181 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.0.181:4317
        }

        chain endpoint-URN53KYD-riset/opentelemetry-collector/tcp/otlp-http__10.42.0.181/4318 {
                ip saddr 10.42.0.181 jump mark-for-masquerade
                meta l4proto tcp dnat to 10.42.0.181:4318
        }
}
```

## Simplified Packet Routing Nftables Rule
```txt
table ip mirzaganteng {
        chain preroute {
                type nat hook prerouting priority mangle; policy accept;
                tcp dport 30000 dnat to 10.42.4.206:8080
        }
}

table ip mirzaganteng { # preset
        chain prerouting { # preset
                type nat hook preroute priority -150; policy accept;
                jump services
        }

        chain services { # preset
                ip daddr @nodeport-ips meta l4proto . th dport vmap @service-nodeports
        }

        set nodeport-ips { # preset
                type ipv4_addr
                comment "IPs that accept NodePort traffic"
                elements = { 10.34.4.246 }
        }

        map service-nodeports { # preset
                type inet_proto . inet_service : verdict
                comment "NodePort traffic"
                elements = {
                        tcp . 30000 : goto wes-mrunuo-hellopod-np-svc,
                        tcp . 30100 : goto wes-mrunuo-mboh-np-svc,
                        tcp . 30200 : goto wes-mrunuo-ikipaling-np-svc,
                }
        }

        chain wes-mrunuo-hellopod-np-svc {
                numgen random mod 100 vmap {
                        0-49  : dnat to 10.42.4.201:8080,  # 50% weight
                        50-74 : dnat to 10.42.4.202:8080,  # 25%
                        75-99 : dnat to 10.42.4.203:8080   # 25%
                }
        }

        chain woi {
                type nat hook prerouting priority filter; policy accept;
                tcp dport 30000 dnat to numgen random mod 3 map { 0 : 192.168.1.0, 1 : 192.168.1.1, 2 : 192.168.1.2 }
        }

nft add rule ip mirzaganteng woi 'dnat to numgen random mod 3 map { 0: 192.168.1.0, 1: 192.168.1.1, 2: 192.168.1.2  }'
}
```

## Log
```txt
2025-11-24T13:38:08.646381Z  INFO proberv2: prober: program starting
2025-11-24T13:38:08.646625Z  INFO proberv2::actor: actor: configuring base nftables ruleset
2025-11-24T13:38:08.652087Z DEBUG proberv2::actor: actor: applying table: Nftables { objects: [CmdObject(Add(Table(Table { family: IP, name: "mirzaganteng", handle: None })))] }
2025-11-24T13:38:08.658427Z DEBUG proberv2::actor: actor: applying set ruleset: Nftables { objects: [CmdObject(Add(Set(Set { family: IP, table: "mirzaganteng", name: "nodeport-ips", handle: None, set_type: Single(Ipv4Addr), policy: None, flags: None, elem: Some([String("10.34.4.141")]), timeout: None, gc_interval: None, size: None, comment: Some("List IPv4 yang nerima traffic dari NodePort") })))] }
2025-11-24T13:38:08.662627Z DEBUG proberv2::actor: actor: applying chain: Nftables { objects: [CmdObject(Add(Chain(Chain { family: IP, table: "mirzaganteng", name: "prerouting", newname: None, handle: None, _type: Some(NAT), hook: Some(Prerouting), prio: Some(-150), dev: None, policy: Some(Accept) }))), CmdObject(Add(Chain(Chain { family: IP, table: "mirzaganteng", name: "services", newname: None, handle: None, _type: None, hook: None, prio: None, dev: None, policy: None })))] }
2025-11-24T13:38:08.666574Z DEBUG proberv2::actor: actor: applying initial ruleset: Nftables { objects: [CmdObject(Add(Rule(Rule { family: IP, table: "mirzaganteng", chain: "prerouting", expr: [Jump(JumpTarget { target: "services" })], handle: Some(0), index: None, comment: None }))), CmdObject(Add(Rule(Rule { family: IP, table: "mirzaganteng", chain: "services", expr: [Match(Match { left: Named(Payload(PayloadField(PayloadField { protocol: "ip", field: "daddr" }))), right: String("@nodeport-ips"), op: EQ }), VerdictMap(VerdictMap { key: Named(Concat([Named(Meta(Meta { key: L4proto })), Named(Payload(PayloadField(PayloadField { protocol: "th", field: "dport" })))])), data: String("@service-verdict-by-nodeport") })], handle: Some(0), index: None, comment: Some("Cek IPv4 paket di list IPv4 NodePort, kalo ada langsung ke verdict map ke service yang sesuai") })))] }
2025-11-24T13:38:08.671079Z  INFO proberv2::actor: actor: starting processes
2025-11-24T13:38:08.674372Z  INFO proberv2::endpoints_watch: actor: endpoints changes occured for hellopod-np-svc service
2025-11-24T13:38:08.675530Z  INFO proberv2::endpoints_watch: actor: captured service hellopod-np-svc endpoints changes: {"k8s-slave-3-raspberrypi-4": ["10.42.1.83", "10.42.1.84", "10.42.1.85", "10.42.1.86"], "k8s-slave-4-raspberrypi-4": ["10.42.5.169", "10.42.5.170", "10.42.5.171", "10.42.5.172", "10.42.5.173"]}
2025-11-24T13:38:08.675550Z  INFO proberv2::endpoints_watch: actor: endpoints changes occured for latency-extender-svc service
2025-11-24T13:38:08.675563Z  INFO proberv2::update_nftables: actor: starting to modify nftables for traffic routing
2025-11-24T13:38:08.675569Z DEBUG proberv2::update_nftables: actor: applying initial ruleset: Nftables { objects: [CmdObject(Add(Chain(Chain { family: IP, table: "mirzaganteng", name: "yowes-ikilo-hellopod-np-svc", newname: None, handle: None, _type: None, hook: None, prio: None, dev: None, policy: None })))] }
2025-11-24T13:38:08.676812Z  INFO proberv2::endpoints_watch: actor: captured service latency-extender-svc endpoints changes: {"k8s-master-1-vm": ["10.42.0.174"]}
2025-11-24T13:38:08.676865Z  INFO proberv2::endpoints_watch: actor: skipping undistributed service latency-extender-svc endpoints containing only 1 node
2025-11-24T13:38:08.676894Z  INFO proberv2::endpoints_watch: actor: endpoints changes occured for opentelemetry-collector service
2025-11-24T13:38:08.678017Z  INFO proberv2::endpoints_watch: actor: captured service opentelemetry-collector endpoints changes: {"k8s-master-1-vm": ["10.42.0.181"]}
2025-11-24T13:38:08.678079Z  INFO proberv2::endpoints_watch: actor: skipping undistributed service opentelemetry-collector endpoints containing only 1 node
2025-11-24T13:38:08.678114Z  INFO proberv2::endpoints_watch: actor: endpoints changes occured for otelcol-opentelemetry-collector service
2025-11-24T13:38:08.678165Z  WARN proberv2::endpoints_watch: actor: empty subsets from endpointslice otelcol-opentelemetry-collector
2025-11-24T13:38:08.679994Z  WARN proberv2::update_nftables: actor: distributed service hellopod-np-svc with only 0 endpoints
2025-11-24T13:38:23.675039Z DEBUG proberv2::latency_probe: actor: latency datapoint calculation result for k8s-slave-3-raspberrypi-4:10.34.4.236 is 0.0012882249999999998
2025-11-24T13:38:23.675690Z DEBUG proberv2::latency_probe: actor: latency datapoint calculation result for k8s-slave-4-raspberrypi-4:10.34.4.208 is 0.0015664849999999998
2025-11-24T13:38:23.676144Z DEBUG proberv2::latency_probe: actor: latency datapoint calculation result for k8s-slave-1-raspberrypi-4:10.34.4.246 is 0.00109776
2025-11-24T13:38:23.676521Z DEBUG proberv2::latency_probe: actor: latency datapoint calculation result for k8s-master-1-vm:10.34.4.142 is 0.000887965
2025-11-24T13:38:23.676562Z DEBUG proberv2::latency_probe: actor: latency datapoint calculation result for k8s-slave-2-vm:10.34.4.141 is 0.000074725
2025-11-24T13:38:23.676574Z  INFO proberv2::actor: actor: updated node k8s-slave-3-raspberrypi-4:10.34.4.236 with latency 0.0012882249999999998 cpu 0
2025-11-24T13:38:23.676578Z  INFO proberv2::actor: actor: updated node k8s-slave-4-raspberrypi-4:10.34.4.208 with latency 0.0015664849999999998 cpu 0
2025-11-24T13:38:23.676581Z  INFO proberv2::actor: actor: updated node k8s-slave-1-raspberrypi-4:10.34.4.246 with latency 0.00109776 cpu 0
2025-11-24T13:38:23.676583Z  INFO proberv2::actor: actor: updated node k8s-master-1-vm:10.34.4.142 with latency 0.000887965 cpu 0
2025-11-24T13:38:23.676586Z  INFO proberv2::actor: actor: updated node k8s-slave-2-vm:10.34.4.141 with latency 0.000074725 cpu 0
2025-11-24T13:38:23.677275Z  WARN proberv2::cpu_usage_probe: actor: empty promql result
2025-11-24T13:38:23.679596Z DEBUG proberv2::cpu_usage_probe: actor: cpu datapoint calculation result for k8s-slave-2-vm:10.34.4.141 is 0.0141851851851853
2025-11-24T13:38:23.679625Z  INFO proberv2::actor: actor: updated node k8s-slave-2-vm:10.34.4.141 with latency 0.000074725 cpu 0.0141851851851853
2025-11-24T13:38:23.680467Z DEBUG proberv2::cpu_usage_probe: actor: cpu datapoint calculation result for k8s-slave-3-raspberrypi-4:10.34.4.236 is 0.08129629629629645
```

## Nftables JSON Request / Response Example
```json
{
  "nftables": [
    {
      "metainfo": {
        "json_schema_version": 1
      }
    },
    {
      "add": {
        "element": {
          "family": "inet",
          "table": "your-table-name",
          "name": "service-nodeports",
          "elem": [
            {
              "key": {
                "concat": [
                  "tcp",
                  30001
                ]
              },
              "val": {
                "goto": {
                  "target": "external-CHAIN123-namespace/service-name/tcp/"
                }
              }
            }
          ]
        }
      }
    }
  ]
}
```
