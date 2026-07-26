#!/usr/bin/env sh

# Resolve paths relative to this script's directory
cd "$(dirname "$0")"

if [ -z "$1" ]; then
    echo "Usage: $0 <environment> <distributions>" >&2
    echo "Example: $0 prod 800,800,800,800" >&2
    exit 1
fi

DISTRIBUTIONS="$1"
SOLUTION="SOLUTION"

mkdir -p dataset

echo "Running testcase=$I with DISTRIBUTIONS=$dists at $(date --iso-8601=minutes)+07:00"

DISTRIBUTIONS="$DISTRIBUTIONS" DURATION="30m" \
  K6_PROMETHEUS_RW_SERVER_URL="${PROMETHEUS_BASE_URL:?set PROMETHEUS_BASE_URL to your Prometheus endpoint}/api/v1/write" \
    k6 run ./traffic-load.js \
    -o experimental-prometheus-rw \
    --no-thresholds
    
