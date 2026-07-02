#!/usr/bin/env sh

if [ -z "$1" ] || [ -z "$2" ]; then
    echo "Usage: $0 <environment> <distributions>" >&2
    echo "Example: $0 prod 800,800,800,800" >&2
    exit 1
fi

DISTRIBUTIONS="$2"
SOLUTION="SOLUTION"

mkdir -p dataset

echo "Running testcase=$I with DISTRIBUTIONS=$dists at $(date --iso-8601=minutes)+07:00"

DISTRIBUTIONS="$DISTRIBUTIONS" DURATION="7m" \
  k6 run \
    --out "csv=dataset/RPS_DATASET_${1}_TESTCASE_0.csv" \
    --no-thresholds \
    ./generation_script.js
