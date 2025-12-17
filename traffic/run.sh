#!/usr/bin/env sh

# COMMA SEPARATED ELEMENT SHOULD MATCH THE NUMBER OF OBJECT IN ./targets.json
DISTRIBUTIONS_LIST="200;400;800;1600;3200;200,200,200,200;400,400,400,400;800,800,800,800;1600,1600,1600,1600;800,400,400,400;1600,400,400,400;3200,400,400,400;3200,200,100,100;1600,800,800,400;1200,800,800,800"
SOLUTION="BASELINE"

I=1

mkdir -p dataset

OLDIFS="$IFS"
IFS=";"
for dists in $DISTRIBUTIONS_LIST
do
  echo "Running testcase=$I with DISTRIBUTIONS=$dists at $(date --iso-8601=minutes)+07:00"

  DISTRIBUTIONS="$dists" DURATION="20m" \
    k6 run \
      --out "csv=dataset/RPS_DATASET_${SOLUTION}_TESTCASE_${I}.csv" \
      --no-thresholds \
      --summary-mode=disabled \
      ./generation_script.js

  I=$((I+1))
done
IFS="$OLDIFS"

