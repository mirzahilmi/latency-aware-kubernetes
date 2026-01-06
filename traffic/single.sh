#!/usr/bin/env sh

DISTRIBUTIONS="3200,400,400,400"
SOLUTION="SOLUTION"

mkdir -p dataset

echo "Running testcase=$I with DISTRIBUTIONS=$dists at $(date --iso-8601=minutes)+07:00"

DISTRIBUTIONS="$DISTRIBUTIONS" DURATION="7m" \
  k6 run \
    --out "csv=dataset/RPS_DATASET_${SOLUTION}_TESTCASE_0.csv" \
    --no-thresholds \
    ./generation_script.js
