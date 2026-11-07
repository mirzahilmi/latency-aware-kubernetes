#!/usr/bin/env sh

cd "$(dirname "$0")"

usage() {
    cat >&2 <<EOF
Usage: $0 <environment> [testcase...]
  environment  BASELINE, IPVS_LC or EWMA (case insensitive)
  testcase     testcase numbers to run, default: all of them
Environment variables:
  REPETITIONS       how many times every testcase runs (default 3); each
                    repetition sweeps all testcases before the next begins
  DURATION_SECONDS  length of each testcase (default 420)
  SEED              arrival trace seed of the first repetition, repetition N
                    uses SEED + N - 1; keep it equal across environments (default 42)
  PROMETHEUS_URL    remote-write to this Prometheus instead of writing CSV,
                    e.g. http://localhost:9090
Example: $0 ewma
Example: PROMETHEUS_URL=http://localhost:9090 $0 ewma 7 8
EOF
}

# Mean req/s per node from docs/TEST_SCENARIOS.md, positional to ./config.json nodes[]
testcase_rates() {
    case "$1" in
        1) echo "200,0,0,0" ;;
        2) echo "1600,0,0,0" ;;
        3) echo "200,200,200,200" ;;
        4) echo "800,800,800,800" ;;
        5) echo "800,400,400,400" ;;
        6) echo "3200,400,400,400" ;;
        7) echo "1600,800,800,400" ;;
        8) echo "1200,800,800,800" ;;
        *) return 1 ;;
    esac
}

if [ $# -lt 1 ] || [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
    usage
    exit 1
fi

fail() {
    echo "Error: $*" >&2
    exit 1
}

CONFIG="config.json"

command -v jq > /dev/null || fail "jq is required to validate $CONFIG"
[ -f "$CONFIG" ] || fail "$CONFIG not found; copy config.json.example to $CONFIG and fill in every worker node"
jq -e . "$CONFIG" > /dev/null 2>&1 || fail "$CONFIG is not valid JSON"

NODE_COUNT="$(jq '.nodes | length' "$CONFIG")"
RATE_COUNT="$(testcase_rates 1 | awk -F, '{ print NF }')"
[ "$NODE_COUNT" = "$RATE_COUNT" ] ||
    fail "$CONFIG has $NODE_COUNT nodes but every testcase sends traffic to $RATE_COUNT"

jq -e '[.nodes[] | .hostname, .ip, .netinterface, .latency] | all(type == "string" and length > 0)' \
    "$CONFIG" > /dev/null ||
    fail "every node in $CONFIG needs a non-empty hostname, ip, netinterface and latency"

for field in hostname ip; do
    jq -e --arg field "$field" '[.nodes[][$field]] | length == (unique | length)' "$CONFIG" > /dev/null ||
        fail "every node in $CONFIG needs a distinct $field (still the config.json.example values?)"
done

# A wrong ip still gets traffic, just not to the node the results are attributed to
jq -r '.nodes[] | "\(.hostname) \(.ip)"' "$CONFIG" | while read -r hostname ip; do
    internal_ip="$(kubectl get node "$hostname" -o jsonpath='{.status.addresses[?(@.type=="InternalIP")].address}')" ||
        fail "$CONFIG hostname $hostname is not a node of the cluster"
    [ "$internal_ip" = "$ip" ] ||
        fail "$CONFIG says $hostname is $ip but its InternalIP is $internal_ip"
done || exit 1

ENVIRONMENT="$(printf '%s' "$1" | tr '[:lower:]' '[:upper:]')"
shift
TESTCASES="${*:-1 2 3 4 5 6 7 8}"

for testcase in $TESTCASES; do
    testcase_rates "$testcase" > /dev/null || {
        echo "Error: unknown testcase: $testcase" >&2
        exit 1
    }
done

REPETITIONS="${REPETITIONS:-3}"
[ "$REPETITIONS" -gt 0 ] 2> /dev/null || fail "REPETITIONS must be a positive integer (got: $REPETITIONS)"

export DURATION_SECONDS="${DURATION_SECONDS:-420}"
FIRST_SEED="${SEED:-42}"

if [ -n "$PROMETHEUS_URL" ]; then
    export K6_PROMETHEUS_RW_SERVER_URL="${PROMETHEUS_URL%/}/api/v1/write"
fi

mkdir -p dataset

# tmux scrollback is unreliable once k6 floods it, so the times also land on disk
RUN_TIMES_CSV="dataset/RUN_TIMES.csv"
[ -f "$RUN_TIMES_CSV" ] || echo "environment,testcase,repetition,started_at,ended_at" > "$RUN_TIMES_CSV"

# Recreates proberv2's nftables table and resets its in-memory EWMA scores, so a
# testcase does not inherit the routing weights of the previous one
restart_proberv2() {
    echo "Restarting proberv2"
    kubectl -n riset rollout restart daemonset/proberv2 &&
        kubectl -n riset rollout status daemonset/proberv2 --timeout=5m
}

for repetition in $(seq 1 "$REPETITIONS"); do
    # A fresh arrival trace per repetition, identical across environments
    export SEED=$((FIRST_SEED + repetition - 1))

    for testcase in $TESTCASES; do
        if [ "$ENVIRONMENT" = "EWMA" ]; then
            restart_proberv2 || fail "proberv2 restart failed before testcase $testcase repetition $repetition"
        fi

        # Cooldown from the previous testcase, and settling time for fresh EWMA scores
        sleep 60

        RATES="$(testcase_rates "$testcase")"
        export RATES

        if [ -n "$PROMETHEUS_URL" ]; then
            set -- -o experimental-prometheus-rw \
                --tag environment="$ENVIRONMENT" \
                --tag testcase="$testcase" \
                --tag repetition="$repetition"
        else
            set -- --out "csv=dataset/RPS_DATASET_${ENVIRONMENT}_TESTCASE_${testcase}_REPETITION_${repetition}.csv"
        fi

        echo "Running testcase $testcase repetition $repetition/$REPETITIONS on $ENVIRONMENT, mean req/s per node: $RATES"
        started_at="$(date --iso-8601=seconds)"
        k6 run "$@" --no-thresholds traffic-load.js
        ended_at="$(date --iso-8601=seconds)"
        echo "$ENVIRONMENT,$testcase,$repetition,$started_at,$ended_at" >> "$RUN_TIMES_CSV"

        # Printed after k6 so the k6 error log can't bury it
        echo "Finished testcase $testcase repetition $repetition started=$started_at ended=$ended_at"
    done
done
