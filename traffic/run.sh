#!/usr/bin/env sh

# COMMA SEPARATED ELEMENT SHOULD MATCH THE NUMBER OF OBJECT IN ./targets.json
DISTRIBUTIONS_LIST="2;4;8;16;32;2,2,2,2;4,4,4,4;8,8,8,8;16,16,16,16;8,4,4,4;16,4,4,4;32,4,4,4;32,2,1,1;16,8,8,4;12,8,8,8"
SOLUTION="BASELINE"

I=1

mkdir -p dataset

OLDIFS="$IFS"
IFS=";"
for dists in $DISTRIBUTIONS_LIST
do
  echo "Running testcase=$I with DISTRIBUTIONS=$dists at $(date --iso-8601=minutes)+07:00"

  DISTRIBUTIONS="$dists" \
    k6 run \
      --out "csv=dataset/RPS_DATASET_${SOLUTION}_TESTCASE_${I}.csv" \
      --no-thresholds \
      --no-summary \
      ./generation_script.js

  I=$((I+1))
done
IFS="$OLDIFS"

