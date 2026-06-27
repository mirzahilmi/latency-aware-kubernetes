#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NODES="${SCRIPT_DIR}/nodes.json"

if [ ! -f "$NODES" ]; then
    echo "Error: config file not found at ${NODES}" >&2
    echo "Config shape: [{\"hostname\": \"...\", \"ip\": \"...\", \"netinterface\": \"...\", \"latency\": \"10ms\"}]" >&2
    exit 1
fi

if ! command -v jq &>/dev/null; then
    echo "Error: jq is required" >&2
    exit 1
fi

NODE_COUNT=$(jq length "$NODES")

for i in $(seq 0 $((NODE_COUNT - 1))); do
    HOSTNAME=$(jq -r ".[$i].hostname" "$NODES")
    IP=$(jq -r ".[$i].ip" "$NODES")
    NIC=$(jq -r ".[$i].netinterface" "$NODES")
    LATENCY=$(jq -r ".[$i].latency" "$NODES")

    echo "==> Setting up node: ${HOSTNAME} (${IP}, nic=${NIC}, latency=${LATENCY})"

    # 1. Label node as worker
    kubectl label node "$HOSTNAME" node-role.kubernetes.io/worker=true --overwrite

    # 2. Label node with primary NIC
    kubectl label node "$HOSTNAME" primary-nic="$NIC" --overwrite

    # 3. Set tc latency for port 30000 on the node
    ssh -t "member@$IP" "\
sudo tc qdisc del dev $NIC root 2>/dev/null || true && \
sudo tc qdisc add dev $NIC root handle 1: prio bands 3 priomap 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 && \
sudo tc qdisc add dev $NIC parent 1:1 handle 10: netem delay $LATENCY && \
sudo tc filter add dev $NIC parent 1:0 protocol ip u32 match ip sport 30000 0xffff flowid 1:1 && \
echo '  tc: $LATENCY latency on port 30000 via $NIC'"

    echo "  done."
done

echo "All nodes configured."
