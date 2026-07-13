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

    # 3. Set tc latency on flannel overlay (all cross-node pod traffic)
    ssh -t "member@$IP" "\
sudo tc qdisc del dev $NIC root 2>/dev/null || true && \
sudo tc qdisc del dev flannel.1 root 2>/dev/null || true && \
sudo tc qdisc add dev flannel.1 root netem delay $LATENCY && \
echo '  tc: $LATENCY latency on flannel.1'"

    echo "  done."
done

echo "All nodes configured."
