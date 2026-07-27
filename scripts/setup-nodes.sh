#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NODES="${SCRIPT_DIR}/config.json"

SSH_USER="member"
DROPIN="/etc/rancher/k3s/config.yaml.d/99-kube-proxy.yaml"
REMOTE_TMP="/tmp/99-kube-proxy.yaml"
MODULES_CONF="/etc/modules-load.d/k3s-ipvs.conf"
IPVS_MODULES="ip_vs ip_vs_lc ip_vs_rr ip_vs_wrr ip_vs_sh nf_conntrack"

usage() {
    echo "Usage: $0 [--proxy-mode iptables|ipvs|nftables] [--ipvs-scheduler NAME] [--no-reboot]" >&2
    echo "  --proxy-mode MODE     kube-proxy datapath for every agent in ./config.json," >&2
    echo "                        written to ${DROPIN}" >&2
    echo "                          iptables  stock kube-proxy default (alias: default)" >&2
    echo "                          ipvs      IPVS, scheduler from --ipvs-scheduler" >&2
    echo "                          nftables  kube-proxy nftables backend (k8s >= 1.31)" >&2
    echo "                        omit to leave the current mode untouched" >&2
    echo "  --ipvs-scheduler S    IPVS scheduler, only with --proxy-mode ipvs" >&2
    echo "                        (default: lc, least connection; also rr, wrr, wlc, sh, ...)" >&2
    echo "  --no-reboot           restart k3s-agent instead of rebooting the node; faster," >&2
    echo "                        but the previous mode may leave stale rules behind" >&2
    echo "  -h, --help            show this help" >&2
    echo "A mode switch reboots each node by default so the old datapath is left clean." >&2
    echo "Node labels and tc latency are applied on every run, after any mode switch." >&2
    echo "Example: $0 --proxy-mode ipvs --ipvs-scheduler lc" >&2
}

PROXY_MODE=""
IPVS_SCHEDULER="lc"
IPVS_SCHEDULER_SET=false
REBOOT=true

while [ $# -gt 0 ]; do
    case "$1" in
        --proxy-mode)
            [ $# -lt 2 ] && { echo "Error: --proxy-mode requires a value" >&2; exit 1; }
            PROXY_MODE="$2"; shift 2 ;;
        --proxy-mode=*)
            PROXY_MODE="${1#--proxy-mode=}"; shift ;;
        --ipvs-scheduler)
            [ $# -lt 2 ] && { echo "Error: --ipvs-scheduler requires a value" >&2; exit 1; }
            IPVS_SCHEDULER="$2"; IPVS_SCHEDULER_SET=true; shift 2 ;;
        --ipvs-scheduler=*)
            IPVS_SCHEDULER="${1#--ipvs-scheduler=}"; IPVS_SCHEDULER_SET=true; shift ;;
        --no-reboot)
            REBOOT=false; shift ;;
        -h|--help)
            usage; exit 0 ;;
        *)
            echo "Error: Unknown argument: $1" >&2; usage; exit 1 ;;
    esac
done

case "$PROXY_MODE" in
    "")
        ;;
    default|iptables)
        PROXY_MODE="iptables" ;;
    ipvs|nftables)
        ;;
    *)
        echo "Error: --proxy-mode must be iptables, ipvs or nftables (got: $PROXY_MODE)" >&2
        exit 1 ;;
esac

if [ "$IPVS_SCHEDULER_SET" = true ] && [ "$PROXY_MODE" != "ipvs" ]; then
    echo "Error: --ipvs-scheduler requires --proxy-mode ipvs" >&2
    exit 1
fi

case "$IPVS_SCHEDULER" in
    rr|wrr|lc|wlc|lblc|lblcr|dh|sh|sed|nq|mh) ;;
    *)
        echo "Error: unknown IPVS scheduler: $IPVS_SCHEDULER" >&2
        exit 1 ;;
esac

if [ ! -f "$NODES" ]; then
    echo "Error: config file not found at ${NODES}" >&2
    echo "Config shape: {\"nodes\": [{\"hostname\": \"...\", \"ip\": \"...\", \"netinterface\": \"...\", \"latency\": \"10ms\"}]}" >&2
    exit 1
fi

if ! command -v jq &>/dev/null; then
    echo "Error: jq is required" >&2
    exit 1
fi

# kube-proxy only grew an nftables backend in 1.31; starting it with an
# unsupported mode leaves the node without service routing.
require_nftables_support() {
    local host="$1" version major minor
    version=$(kubectl get node "$host" -o jsonpath='{.status.nodeInfo.kubeletVersion}' 2>/dev/null || true)
    if [ -z "$version" ]; then
        echo "  warn: cannot read kubelet version for ${host}, skipping nftables support check" >&2
        return 0
    fi
    version="${version#v}"
    major="${version%%.*}"
    minor="${version#*.}"
    minor="${minor%%.*}"
    case "${major}.${minor}" in
        *[!0-9.]*)
            echo "  warn: cannot parse kubelet version ${version} for ${host}, skipping check" >&2
            return 0 ;;
    esac
    if [ "$major" -eq 1 ] && [ "$minor" -lt 31 ]; then
        echo "Error: ${host} runs kubelet ${version}; proxy-mode=nftables needs k8s >= 1.31" >&2
        exit 1
    fi
}

DROPIN_FILE=""
build_dropin() {
    DROPIN_FILE=$(mktemp)
    trap 'rm -f "$DROPIN_FILE"' EXIT
    {
        echo "# Managed by scripts/setup-nodes.sh -- see https://docs.k3s.io/cli/agent"
        echo "---"
        echo "kube-proxy-arg:"
        echo "  - proxy-mode=${PROXY_MODE}"
        if [ "$PROXY_MODE" = "ipvs" ]; then
            echo "  - ipvs-scheduler=${IPVS_SCHEDULER}"
        fi
    } >"$DROPIN_FILE"
}

node_ready() {
    [ "$(kubectl get "node/$1" -o jsonpath='{.status.conditions[?(@.type=="Ready")].status}' 2>/dev/null)" = "True" ]
}

apply_proxy_mode() {
    local host="$1" ip="$2" steps

    # Numbered 99- so it sorts last and its kube-proxy-arg wins over any
    # earlier drop-in; anything else still setting the flag is reported.
    steps="sudo install -D -m 0644 ${REMOTE_TMP} ${DROPIN} && rm -f ${REMOTE_TMP}"
    steps="${steps} && sudo grep -ls kube-proxy-arg /etc/rancher/k3s/config.yaml /etc/rancher/k3s/config.yaml.d/*.yaml"
    steps="${steps} | grep -v '99-kube-proxy.yaml' | sed 's|^|  warn: also sets kube-proxy-arg: |'"

    if [ "$PROXY_MODE" = "ipvs" ]; then
        steps="${steps} && printf '%s\\n' ${IPVS_MODULES} | sudo tee ${MODULES_CONF} >/dev/null"
        steps="${steps} && sudo modprobe -a ${IPVS_MODULES}"
    fi

    scp -q "$DROPIN_FILE" "${SSH_USER}@${ip}:${REMOTE_TMP}"

    if [ "$REBOOT" = true ]; then
        # The reboot kills the connection, so ssh's own exit status is noise.
        ssh -t "${SSH_USER}@${ip}" "${steps} && sudo systemctl reboot" || true
        echo "  rebooting ${host}..."
        # The control plane only reports NotReady after the kubelet grace
        # period; waiting for Ready before that would match the stale status.
        for _ in $(seq 1 36); do
            sleep 5
            node_ready "$host" || break
        done
    else
        ssh -t "${SSH_USER}@${ip}" "${steps} && \
if systemctl cat k3s-agent >/dev/null 2>&1; then sudo systemctl restart k3s-agent; else sudo systemctl restart k3s; fi"
        echo "  k3s agent restarted"
        sleep 15
    fi

    kubectl wait --for=condition=Ready "node/${host}" --timeout=600s
}

if [ -n "$PROXY_MODE" ]; then
    build_dropin
fi

NODE_COUNT=$(jq '.nodes | length' "$NODES")

for i in $(seq 0 $((NODE_COUNT - 1))); do
    HOSTNAME=$(jq -r ".nodes[$i].hostname" "$NODES")
    IP=$(jq -r ".nodes[$i].ip" "$NODES")
    NIC=$(jq -r ".nodes[$i].netinterface" "$NODES")
    LATENCY=$(jq -r ".nodes[$i].latency" "$NODES")

    echo "==> Setting up node: ${HOSTNAME} (${IP}, nic=${NIC}, latency=${LATENCY})"

    # 1. Switch the kube-proxy datapath, if asked; do it first because the
    #    agent restart (or reboot) below wipes the tc qdisc set further down.
    if [ -n "$PROXY_MODE" ]; then
        if [ "$PROXY_MODE" = "nftables" ]; then
            require_nftables_support "$HOSTNAME"
        fi
        if [ "$PROXY_MODE" = "ipvs" ]; then
            echo "  kube-proxy: proxy-mode=${PROXY_MODE} ipvs-scheduler=${IPVS_SCHEDULER}"
        else
            echo "  kube-proxy: proxy-mode=${PROXY_MODE}"
        fi
        apply_proxy_mode "$HOSTNAME" "$IP"
    fi

    # 2. Label node as worker
    kubectl label node "$HOSTNAME" node-role.kubernetes.io/worker=true --overwrite

    # 3. Label node with primary NIC
    kubectl label node "$HOSTNAME" primary-nic="$NIC" --overwrite

    # 4. Set tc latency on flannel overlay (all cross-node pod traffic)
    ssh -t "${SSH_USER}@${IP}" "\
sudo tc qdisc del dev $NIC root 2>/dev/null || true && \
sudo tc qdisc del dev flannel.1 root 2>/dev/null || true && \
sudo tc qdisc add dev flannel.1 root netem delay $LATENCY && \
echo '  tc: $LATENCY latency on flannel.1'"

    echo "  done."
done

echo "All nodes configured."
