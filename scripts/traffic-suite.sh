#!/usr/bin/env sh

# Resolve paths relative to this script's directory
cd "$(dirname "$0")"

# Semicolon separated testcases; within a testcase, COMMA SEPARATED ELEMENT
# SHOULD MATCH THE NUMBER OF OBJECT IN ./config.json nodes[]
DEFAULT_SCENARIOS="200;1600;200,200,200,200;800,800,800,800;800,400,400,400;3200,400,400,400;1600,800,800,400;1200,800,800,800"

usage() {
    echo "Usage: $0 <environment> [--mode constant|poisson] [--seed N] [--scenarios LIST]" >&2
    echo "                        [--duration D] [--output csv|prometheus] [--prometheus-url URL]" >&2
    echo "  <environment>     uppercased; analysis/ expects BASELINE, IPVS_LC or SOLUTION" >&2
    echo "  --mode constant   evenly paced arrivals (default, ./traffic-load.js)" >&2
    echo "  --mode poisson    Poisson arrivals at the same mean rate (./traffic-load-poisson.js)" >&2
    echo "  --seed N          PRNG seed for --mode poisson (default 42)" >&2
    echo "  --scenarios LIST  semicolon separated testcases, each a comma separated" >&2
    echo "                    req/s per node, positional to ./config.json nodes[]" >&2
    echo "                    (default: $DEFAULT_SCENARIOS)" >&2
    echo "  --duration D      k6 duration per testcase (default 7m)" >&2
    echo "  --output csv      write dataset/RPS_DATASET_<ENV>_TESTCASE_<N>.csv (default)" >&2
    echo "  --output prometheus  remote-write, tagged environment/mode/testcase;" >&2
    echo "                    requires --prometheus-url" >&2
    echo "  --prometheus-url URL  Prometheus base URL, /api/v1/write is appended" >&2
    echo "                        (e.g. http://localhost:9090)" >&2
    echo "Example: $0 solution --mode poisson --scenarios '800,800,800,800;1600,800,800,400'" >&2
}

ENVIRONMENT=""
MODE="constant"
SEED="42"
SCENARIOS="$DEFAULT_SCENARIOS"
DURATION="7m"
OUTPUT="csv"
PROMETHEUS_URL=""

while [ $# -gt 0 ]; do
    case "$1" in
        --mode)
            [ $# -lt 2 ] && { echo "Error: --mode requires a value" >&2; exit 1; }
            MODE="$2"; shift 2 ;;
        --mode=*)
            MODE="${1#--mode=}"; shift ;;
        --seed)
            [ $# -lt 2 ] && { echo "Error: --seed requires a value" >&2; exit 1; }
            SEED="$2"; shift 2 ;;
        --seed=*)
            SEED="${1#--seed=}"; shift ;;
        --scenarios)
            [ $# -lt 2 ] && { echo "Error: --scenarios requires a value" >&2; exit 1; }
            SCENARIOS="$2"; shift 2 ;;
        --scenarios=*)
            SCENARIOS="${1#--scenarios=}"; shift ;;
        --duration)
            [ $# -lt 2 ] && { echo "Error: --duration requires a value" >&2; exit 1; }
            DURATION="$2"; shift 2 ;;
        --duration=*)
            DURATION="${1#--duration=}"; shift ;;
        --output)
            [ $# -lt 2 ] && { echo "Error: --output requires a value" >&2; exit 1; }
            OUTPUT="$2"; shift 2 ;;
        --output=*)
            OUTPUT="${1#--output=}"; shift ;;
        --prometheus-url)
            [ $# -lt 2 ] && { echo "Error: --prometheus-url requires a value" >&2; exit 1; }
            PROMETHEUS_URL="$2"; shift 2 ;;
        --prometheus-url=*)
            PROMETHEUS_URL="${1#--prometheus-url=}"; shift ;;
        -h|--help)
            usage; exit 0 ;;
        -*)
            echo "Error: Unknown option: $1" >&2; usage; exit 1 ;;
        *)
            if [ -z "$ENVIRONMENT" ]; then
                ENVIRONMENT="$1"
            else
                echo "Error: Unexpected argument: $1" >&2; usage; exit 1
            fi
            shift ;;
    esac
done

if [ -z "$ENVIRONMENT" ]; then
    echo "Error: Missing required argument: environment" >&2
    usage
    exit 1
fi

ENVIRONMENT="$(printf '%s' "$ENVIRONMENT" | tr '[:lower:]' '[:upper:]')"

case "$MODE" in
    constant)
        SCRIPT="./traffic-load.js"
        SUFFIX="" ;;
    poisson)
        SCRIPT="./traffic-load-poisson.js"
        SUFFIX="_POISSON" ;;
    *)
        echo "Error: --mode must be constant or poisson (got: $MODE)" >&2
        exit 1 ;;
esac

case "$DURATION" in
    "")
        echo "Error: --duration must not be empty" >&2; exit 1 ;;
    *[!0-9hms]*)
        echo "Error: --duration must be a k6 duration like 7m, 30m or 1h30m (got: $DURATION)" >&2; exit 1 ;;
    [!0-9]*)
        echo "Error: --duration must start with a number (got: $DURATION)" >&2; exit 1 ;;
    *[0-9])
        echo "Error: --duration is missing a unit (got: $DURATION)" >&2; exit 1 ;;
esac

RW_URL=""
case "$OUTPUT" in
    csv)
        if [ -n "$PROMETHEUS_URL" ]; then
            echo "Error: --prometheus-url requires --output prometheus" >&2
            exit 1
        fi ;;
    prometheus)
        if [ -z "$PROMETHEUS_URL" ]; then
            echo "Error: --output prometheus requires --prometheus-url" >&2
            exit 1
        fi
        # Tolerate a trailing slash so the write path is never doubled up
        RW_URL="${PROMETHEUS_URL%/}/api/v1/write" ;;
    *)
        echo "Error: --output must be csv or prometheus (got: $OUTPUT)" >&2
        exit 1 ;;
esac

if [ -z "$SCENARIOS" ]; then
    echo "Error: --scenarios must not be empty" >&2
    exit 1
fi

# Fail before burning a k6 startup on a malformed testcase
OLDIFS="$IFS"
IFS=";"
for dists in $SCENARIOS
do
  IFS="$OLDIFS"
  case "$dists" in
      "")
          echo "Error: --scenarios contains an empty testcase: $SCENARIOS" >&2; exit 1 ;;
      *[!0-9,]*)
          echo "Error: --scenarios testcase must be comma separated numbers (got: $dists)" >&2; exit 1 ;;
      ,*|*,|*,,*)
          echo "Error: --scenarios testcase has an empty rate (got: $dists)" >&2; exit 1 ;;
  esac
  IFS=";"
done
IFS="$OLDIFS"

I=1

mkdir -p dataset

# tmux scrollback is unreliable once k6 floods it, so the times also land on disk.
# Global across every invocation of this script; the counter picks up where the
# previous run left off.
TIMES_CSV="dataset/TESTCASE_TIMES.csv"
[ -f "$TIMES_CSV" ] || echo "testcase,started_at,ended_at" > "$TIMES_CSV"

RUN_NO="$(tail -n 1 "$TIMES_CSV" | cut -d, -f1)"
case "$RUN_NO" in
    ""|*[!0-9]*) RUN_NO=0 ;;
esac

IFS=";"
for dists in $SCENARIOS
do
  IFS="$OLDIFS"

  echo "Running testcase=$I env=$ENVIRONMENT mode=$MODE output=$OUTPUT duration=$DURATION with DISTRIBUTIONS=$dists"

  STARTED_AT="$(date --iso-8601=seconds)"

  # Positional params are free here; the option loop above already consumed them.
  if [ "$OUTPUT" = "csv" ]; then
      set -- --out "csv=dataset/RPS_DATASET_${ENVIRONMENT}${SUFFIX}_TESTCASE_${I}.csv"
  else
      set -- -o experimental-prometheus-rw \
          --tag environment="$ENVIRONMENT" \
          --tag mode="$MODE" \
          --tag testcase="$I"
  fi

  DISTRIBUTIONS="$dists" DURATION="$DURATION" SEED="$SEED" \
    K6_PROMETHEUS_RW_SERVER_URL="$RW_URL" \
      k6 run \
        "$@" \
        --no-thresholds \
        "$SCRIPT"

  ENDED_AT="$(date --iso-8601=seconds)"
  RUN_NO=$((RUN_NO+1))
  echo "$RUN_NO,$STARTED_AT,$ENDED_AT" >> "$TIMES_CSV"

  # Printed after k6 so the k6 error log can't bury it
  echo "Finished testcase=$I started=$STARTED_AT ended=$ENDED_AT"

  I=$((I+1))

  sleep $((1*60))

  IFS=";"
done
IFS="$OLDIFS"
