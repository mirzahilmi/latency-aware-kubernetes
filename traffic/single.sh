#!/usr/bin/env sh

if [ -z "$1" ]; then
    echo "Error: Missing required argument: environment" >&2
    exit 1
fi

DISTRIBUTIONS="3200,400,400,400"
SOLUTION="SOLUTION"

mkdir -p dataset

echo "Running testcase=$I with DISTRIBUTIONS=$dists at $(date --iso-8601=minutes)+07:00"

DISTRIBUTIONS="$DISTRIBUTIONS" DURATION="7m" \
  k6 run \
    --out "csv=dataset/RPS_DATASET_${1}_TESTCASE_0.csv" \
    --no-thresholds \
    ./generation_script.js
