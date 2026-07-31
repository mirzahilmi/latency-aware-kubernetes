#!/usr/bin/env sh

# Resolve paths relative to this script's directory
cd "$(dirname "$0")"

# Semicolon separated testcases; within a testcase, COMMA SEPARATED ELEMENT
# SHOULD MATCH THE NUMBER OF OBJECT IN ./config.json nodes[]
DEFAULT_SCENARIOS="200;1600;200,200,200,200;800,800,800,800;800,400,400,400;3200,400,400,400;1600,800,800,400;1200,800,800,800"

# Staircase profile for --mode staircase: one run that climbs to a peak and
# returns, holding the 4:2:2:1 ratio of testcase 7 the whole way. Levels are that
# vector scaled 1x..3x, peaking at 2700 req/s total. 5 steps, 15 minutes, 2
# hysteresis pairs.
#
# One shape throughout is deliberate: every transition then changes only the
# offered load, so a knee between two steps is attributable to load rather than
# to load and distribution moving together.
#
# The peak is capped by the LOAD GENERATOR, not by the cluster. A 27m run peaking
# at 4500 req/s was OOM-killed at 18m54s having reserved 14607 VUs, and from
# ~590s in it was also logging "dial: i/o timeout" against a worker: at ~3000
# connections/s (noVUConnectionReuse is on, so every iteration opens one)
# nf_conntrack overflows and SYNs are dropped silently. Both failures are in the
# measuring apparatus, so anything above ~2700 req/s measures the harness rather
# than the scheduler. Raise this only after confirming conntrack headroom and
# available RAM on the k6 host.
#
# Step duration is bounded from below by the prober's control loop: with
# latencyInterval 5s and alpha 0.3 the EWMA time constant is ~17s, so ~50s of
# settling plus up to nftUpdateInterval (10s) of nftables staleness means a step
# shorter than ~3m has no settled window left after the warmup discard. Cut
# steps, not step duration.
#
# Wider profiles, all via --steps, in ascending order of what they demand from
# the load generator. Each was reasoned about but NONE has been shown to survive
# on this hardware -- check the VU allocation k6 logs at startup before using one:
#
# 7 steps, 21 min/env, peak 3600 (== testcase 7):
#   400,200,200,100;800,400,400,200;1200,600,600,300;1600,800,800,400;\
#   1200,600,600,300;800,400,400,200;400,200,200,100
#
# 9 steps, 27 min/env, peak 4500 -- THIS IS THE PROFILE THAT WAS OOM-KILLED:
#   400,200,200,100;800,400,400,200;1200,600,600,300;1600,800,800,400;\
#   2000,1000,1000,500;1600,800,800,400;1200,600,600,300;800,400,400,200;\
#   400,200,200,100
#
# 15 steps, 45 min/env -- walks all eight DEFAULT_SCENARIOS testcases up and back
# down, so its up leg lines up row for row with the eight ratio rows of
# TEMPLATE_TABEL_PARAMETER_EVALUASI.xlsx. TC1 and TC2 drive one entry node,
# written with explicit zeros exactly as the template spells them ("200:0:0:0").
# Its testcase 6 step puts 3200 req/s on a single node, well past where the dial
# timeouts began:
#   200,0,0,0;200,200,200,200;1600,0,0,0;800,400,400,400;800,800,800,800;\
#   1600,800,800,400;1200,800,800,800;3200,400,400,400;1200,800,800,800;\
#   1600,800,800,400;800,800,800,800;800,400,400,400;1600,0,0,0;\
#   200,200,200,200;200,0,0,0
DEFAULT_STEPS="400,200,200,100;800,400,400,200;1200,600,600,300;800,400,400,200;400,200,200,100"
DEFAULT_STEP_DURATION="3m"
DEFAULT_WARMUP="60s"

usage() {
    echo "Usage: $0 <environment> [--mode constant|poisson|staircase] [--seed N]" >&2
    echo "                        [--scenarios LIST] [--duration D]" >&2
    echo "                        [--steps LIST] [--step-duration D] [--warmup D]" >&2
    echo "                        [--output csv|prometheus] [--prometheus-url URL]" >&2
    echo "  <environment>     uppercased; analysis/ expects BASELINE, IPVS_LC or EWMA" >&2
    echo "  --mode constant   evenly paced arrivals (default, ./traffic-load.js)" >&2
    echo "  --mode poisson    Poisson arrivals at the same mean rate, one run per" >&2
    echo "                    testcase (./traffic-load-poisson.js)" >&2
    echo "  --mode staircase  ONE Poisson run whose mean rate steps up then back" >&2
    echo "                    down; replaces the whole --scenarios sweep" >&2
    echo "  --seed N          PRNG seed for poisson and staircase (default 42)" >&2
    echo "  --scenarios LIST  constant/poisson only: semicolon separated testcases," >&2
    echo "                    each a comma separated req/s per node, positional to" >&2
    echo "                    ./config.json nodes[] (default: $DEFAULT_SCENARIOS)" >&2
    echo "  --duration D      constant/poisson only: k6 duration per testcase (default 7m)" >&2
    echo "  --steps LIST      staircase only: same syntax as --scenarios, but the" >&2
    echo "                    entries are consecutive steps of a single run" >&2
    echo "                    (default: $DEFAULT_STEPS)" >&2
    echo "  --step-duration D staircase only: k6 duration held per step (default $DEFAULT_STEP_DURATION)" >&2
    echo "  --warmup D        staircase only: leading slice of each step to discard" >&2
    echo "                    when aggregating; recorded in the schedule CSV, k6" >&2
    echo "                    still runs it (default $DEFAULT_WARMUP)" >&2
    echo "  --vu-factor N     poisson/staircase only: VUs reserved per req/s of peak" >&2
    echo "                    rate, i.e. a response time budget in seconds (default 1)." >&2
    echo "                    Every VU is a JS runtime and the pool is held for the" >&2
    echo "                    whole run, so a loose factor gets long runs OOM-killed." >&2
    echo "                    k6 logs the resolved allocation before allocating it." >&2
    echo "  --output csv      write dataset/RPS_DATASET_<ENV>_TESTCASE_<N>.csv, or" >&2
    echo "                    dataset/RPS_DATASET_<ENV>_STAIRCASE.csv (default)" >&2
    echo "  --output prometheus  remote-write, tagged environment/mode/testcase;" >&2
    echo "                    requires --prometheus-url" >&2
    echo "  --prometheus-url URL  Prometheus base URL, /api/v1/write is appended" >&2
    echo "                        (e.g. http://localhost:9090)" >&2
    echo "Example: $0 ewma --mode poisson --scenarios '800,800,800,800;1600,800,800,400'" >&2
    echo "Example: $0 ewma --mode staircase" >&2
}

# k6 duration text -> whole seconds. Rejects anything the suite would otherwise
# hand to k6 and only find out about after paying for a startup.
duration_seconds() {
    printf '%s' "$1" | awk '{
        total = 0
        digits = ""
        for (i = 1; i <= length($0); i++) {
            c = substr($0, i, 1)
            if (c ~ /[0-9]/) { digits = digits c; continue }
            if (digits == "") exit 1
            if (c == "h") total += digits * 3600
            else if (c == "m") total += digits * 60
            else if (c == "s") total += digits
            else exit 1
            digits = ""
        }
        if (digits != "") exit 1
        if (total <= 0) exit 1
        printf "%d", total
    }'
}

validate_duration() {
    case "$2" in
        "")
            echo "Error: $1 must not be empty" >&2; exit 1 ;;
        *[!0-9hms]*)
            echo "Error: $1 must be a k6 duration like 7m, 30m or 1h30m (got: $2)" >&2; exit 1 ;;
        [!0-9]*)
            echo "Error: $1 must start with a number (got: $2)" >&2; exit 1 ;;
        *[0-9])
            echo "Error: $1 is missing a unit (got: $2)" >&2; exit 1 ;;
    esac
}

validate_rate_list() {
    OLDIFS="$IFS"
    IFS=";"
    for entry in $2
    do
      IFS="$OLDIFS"
      case "$entry" in
          "")
              echo "Error: $1 contains an empty entry: $2" >&2; exit 1 ;;
          *[!0-9,]*)
              echo "Error: $1 entries must be comma separated numbers (got: $entry)" >&2; exit 1 ;;
          ,*|*,|*,,*)
              echo "Error: $1 entry has an empty rate (got: $entry)" >&2; exit 1 ;;
      esac
      IFS=";"
    done
    IFS="$OLDIFS"
}

# Emits the step schedule the analysis side needs to slice one staircase CSV
# back into per-step windows: offsets are relative to the run's first sample.
# The peak splits the run into legs; a level before it is on the way up.
generate_schedule() {
    printf '%s' "$1" | awk -v step_seconds="$2" -v warmup_seconds="$3" '
    BEGIN { RS = ";"; count = 0 }
    {
        gsub(/[ \t\r\n]/, "", $0)
        if ($0 == "") next
        count++
        split($0, rates, ",")
        total = 0
        ratio = ""
        for (i = 1; i in rates; i++) {
            total += rates[i]
            ratio = ratio (i > 1 ? ":" : "") rates[i]
        }
        totals[count] = total
        ratios[count] = ratio
    }
    END {
        peak = 0
        peak_step = 0
        for (i = 1; i <= count; i++) if (totals[i] > peak) { peak = totals[i]; peak_step = i }
        print "step,leg,rps_per_node,total_rps,start_offset_s,end_offset_s,warmup_s"
        for (i = 1; i <= count; i++) {
            leg = (i < peak_step) ? "up" : ((i == peak_step) ? "peak" : "down")
            printf "%d,%s,%s,%d,%d,%d,%d\n", \
                i, leg, ratios[i], totals[i], \
                (i - 1) * step_seconds, i * step_seconds, warmup_seconds
        }
    }'
}

ENVIRONMENT=""
MODE="constant"
SEED="42"
SCENARIOS="$DEFAULT_SCENARIOS"
DURATION="7m"
DURATION_SET=0
STEPS="$DEFAULT_STEPS"
STEP_DURATION="$DEFAULT_STEP_DURATION"
WARMUP="$DEFAULT_WARMUP"
VU_FACTOR="1"
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
            DURATION="$2"; DURATION_SET=1; shift 2 ;;
        --duration=*)
            DURATION="${1#--duration=}"; DURATION_SET=1; shift ;;
        --steps)
            [ $# -lt 2 ] && { echo "Error: --steps requires a value" >&2; exit 1; }
            STEPS="$2"; shift 2 ;;
        --steps=*)
            STEPS="${1#--steps=}"; shift ;;
        --step-duration)
            [ $# -lt 2 ] && { echo "Error: --step-duration requires a value" >&2; exit 1; }
            STEP_DURATION="$2"; shift 2 ;;
        --step-duration=*)
            STEP_DURATION="${1#--step-duration=}"; shift ;;
        --warmup)
            [ $# -lt 2 ] && { echo "Error: --warmup requires a value" >&2; exit 1; }
            WARMUP="$2"; shift 2 ;;
        --warmup=*)
            WARMUP="${1#--warmup=}"; shift ;;
        --vu-factor)
            [ $# -lt 2 ] && { echo "Error: --vu-factor requires a value" >&2; exit 1; }
            VU_FACTOR="$2"; shift 2 ;;
        --vu-factor=*)
            VU_FACTOR="${1#--vu-factor=}"; shift ;;
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
    staircase)
        SCRIPT="./traffic-load-poisson.js"
        SUFFIX="" ;;
    *)
        echo "Error: --mode must be constant, poisson or staircase (got: $MODE)" >&2
        exit 1 ;;
esac

if [ "$MODE" = "staircase" ]; then
    # --duration would look like it sets the total run length; it does not. The
    # run is exactly (number of steps) * --step-duration.
    if [ "$DURATION_SET" = "1" ]; then
        echo "Error: --duration does not apply to --mode staircase; use --step-duration" >&2
        exit 1
    fi

    validate_duration "--step-duration" "$STEP_DURATION"
    validate_duration "--warmup" "$WARMUP"

    if [ -z "$STEPS" ]; then
        echo "Error: --steps must not be empty" >&2
        exit 1
    fi
    validate_rate_list "--steps" "$STEPS"

    STEP_SECONDS="$(duration_seconds "$STEP_DURATION")" || {
        echo "Error: could not parse --step-duration: $STEP_DURATION" >&2; exit 1; }
    WARMUP_SECONDS="$(duration_seconds "$WARMUP")" || {
        echo "Error: could not parse --warmup: $WARMUP" >&2; exit 1; }

    if [ "$WARMUP_SECONDS" -ge "$STEP_SECONDS" ]; then
        echo "Error: --warmup ($WARMUP) leaves no settled window inside a --step-duration of $STEP_DURATION" >&2
        exit 1
    fi
else
    validate_duration "--duration" "$DURATION"

    if [ -z "$SCENARIOS" ]; then
        echo "Error: --scenarios must not be empty" >&2
        exit 1
    fi
    # Fail before burning a k6 startup on a malformed testcase
    validate_rate_list "--scenarios" "$SCENARIOS"
fi

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

OLDIFS="$IFS"

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

# $1 = filename label (TESTCASE_3, STAIRCASE), $2 = prometheus testcase tag,
# $3 = what to echo about this run. Arrival rates are read from the environment,
# so callers must export either DISTRIBUTIONS+DURATION or STEPS+STEP_DURATION
# before calling. A `VAR=x run_k6 ...` prefix would not do: for a function those
# assignments are not marked for export, so k6 would never see them.
run_k6() {
  echo "Running $3"

  # Copied out before the `set --` below reassigns the positional params.
  LABEL="$1"

  STARTED_AT="$(date --iso-8601=seconds)"

  # Positional params are free here; the option loop above already consumed them.
  if [ "$OUTPUT" = "csv" ]; then
      set -- --out "csv=dataset/RPS_DATASET_${ENVIRONMENT}${SUFFIX}_${LABEL}.csv"
  else
      set -- -o experimental-prometheus-rw \
          --tag environment="$ENVIRONMENT" \
          --tag mode="$MODE" \
          --tag testcase="$2"
  fi

  K6_PROMETHEUS_RW_SERVER_URL="$RW_URL" \
      k6 run \
        "$@" \
        --no-thresholds \
        "$SCRIPT"

  ENDED_AT="$(date --iso-8601=seconds)"
  RUN_NO=$((RUN_NO+1))
  echo "$RUN_NO,$STARTED_AT,$ENDED_AT" >> "$TIMES_CSV"

  # Printed after k6 so the k6 error log can't bury it
  echo "Finished $LABEL started=$STARTED_AT ended=$ENDED_AT"
}

if [ "$MODE" = "staircase" ]; then
    STEP_COUNT="$(printf '%s' "$STEPS" | tr ';' '\n' | grep -c '[0-9]')"
    TOTAL_SECONDS=$((STEP_COUNT * STEP_SECONDS))

    # The schedule is what turns one CSV back into per-step windows, so every
    # configuration has to be sliced by the same one. Refuse to silently replace
    # a schedule a previous environment was already measured against.
    SCHEDULE_CSV="dataset/STAIRCASE_STEPS.csv"
    generate_schedule "$STEPS" "$STEP_SECONDS" "$WARMUP_SECONDS" > "$SCHEDULE_CSV.new"
    if [ -f "$SCHEDULE_CSV" ] && ! cmp -s "$SCHEDULE_CSV" "$SCHEDULE_CSV.new"; then
        rm -f "$SCHEDULE_CSV.new"
        echo "Error: $SCHEDULE_CSV describes a different staircase than the one requested." >&2
        echo "       Runs sliced by different schedules are not comparable. Delete it if" >&2
        echo "       you are starting a fresh set of runs." >&2
        exit 1
    fi
    mv "$SCHEDULE_CSV.new" "$SCHEDULE_CSV"

    echo "Staircase: $STEP_COUNT steps of $STEP_DURATION = ${TOTAL_SECONDS}s total, warmup $WARMUP discarded per step"
    echo "Schedule written to $SCHEDULE_CSV"

    export STEPS STEP_DURATION SEED VU_FACTOR
    run_k6 "STAIRCASE" "staircase" \
        "staircase env=$ENVIRONMENT output=$OUTPUT step-duration=$STEP_DURATION with STEPS=$STEPS"
else
    # STEPS still holds its default here; leaving it exported would make
    # traffic-load-poisson.js prefer it over DISTRIBUTIONS and quietly run a
    # staircase instead of the requested testcase.
    unset STEPS STEP_DURATION

    export DURATION SEED VU_FACTOR

    I=1
    IFS=";"
    for dists in $SCENARIOS
    do
      IFS="$OLDIFS"

      export DISTRIBUTIONS="$dists"

      run_k6 "TESTCASE_${I}" "$I" \
        "testcase=$I env=$ENVIRONMENT mode=$MODE output=$OUTPUT duration=$DURATION with DISTRIBUTIONS=$dists"

      I=$((I+1))

      sleep $((1*60))

      IFS=";"
    done
    IFS="$OLDIFS"
fi
