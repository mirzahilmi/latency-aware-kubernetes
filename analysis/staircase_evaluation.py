"""Aggregate one staircase run per configuration into per-step evaluation rows.

A staircase run (scripts/traffic-suite.sh --mode staircase) replaces the whole
--scenarios sweep with a single k6 run whose mean arrival rate steps up to a peak
and back down. That means the per-testcase-file aggregation of
fill_evaluation_table.py no longer applies: every load level lives inside ONE
CSV, delimited by time rather than by filename.

This script slices each CSV back into per-step windows using the schedule
scripts/traffic-suite.sh wrote alongside the run (STAIRCASE_STEPS.csv), and
reports the same three statistics as fill_evaluation_table.py plus the two that
only a staircase can produce:

    WAKTU RESPON              (http_req_duration) -> arithmetic MEAN            [ms]
    THROUGHPUT                (http_reqs)         -> COUNT / window             [req/s]
    TINGKAT KEGAGALAN REQUEST (http_req_failed)   -> 100 * MEAN                 [%]
    p95 / p99 response time   (http_req_duration) -> approximate quantiles      [ms]
    DROPPED ITERATIONS        (dropped_iterations)-> SUM                        [count]

Dropped iterations are the saturation signal of an open workload model: once the
cluster cannot keep up, k6 cannot start the arrivals the schedule called for, and
those never appear in http_req_duration at all. Reading mean response time
without it understates saturation.

Why the two extra outputs matter:

  - Throughput uses the DESIGNED window length, not (max - min) of the observed
    timestamps. Under saturation a configuration emits fewer samples over the
    same wall clock; dividing by the observed span would hide exactly that.

  - Because the staircase visits every level except the peak twice, the same
    offered load is measured once while climbing and once while descending. The
    gap between the two is hysteresis: how much of the control loop's state is
    still carrying the previous, heavier load. A stateless load balancer shows
    ~0; an EWMA-smoothed one shows whatever its damping costs.

Both windows discard the leading warmup_s of their step (recorded in the
schedule). With latencyInterval 5s and alpha 0.3 the prober's EWMA time constant
is ~17s, so ~50s of settling plus up to nftUpdateInterval of nftables staleness
has to be thrown away before a step is in steady state.

Pipeline mirrors fill_evaluation_table.py: DuckDB streams the CSVs (they run to
tens of millions of rows each) and only the small result frame reaches Python.
Two scans total -- one for each file's time origin, one for every metric at once.

Outputs STAIRCASE_EVALUATION.csv and STAIRCASE_HYSTERESIS.csv next to this
script, and prints both tables.
"""

from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path

import duckdb

WORKING_DIRECTORY = Path(__file__).resolve().parent

DEFAULT_CONFIGURATIONS = ["EWMA", "BASELINE", "IPVS_LC"]

# Filename configuration token -> label used in the printed tables, matching the
# column meanings of TEMPLATE_TABEL_PARAMETER_EVALUASI.xlsx.
CONFIGURATION_TO_LABEL = {
    "EWMA":     "EWMA",      # proposed solution
    "BASELINE": "BASELINE",
    "IPVS_LC":  "LEASTCON",
}

EVALUATION_OUTPUT = WORKING_DIRECTORY / "STAIRCASE_EVALUATION.csv"
HYSTERESIS_OUTPUT = WORKING_DIRECTORY / "STAIRCASE_HYSTERESIS.csv"

EVALUATION_COLUMNS = [
    "configuration",
    "step",
    "leg",
    "rps_per_node",
    "offered_total_rps",
    "window_s",
    "mean_response_time_ms",
    "p95_response_time_ms",
    "p99_response_time_ms",
    "throughput_rps",
    "failure_rate_percent",
    "total_requests",
    "dropped_iterations",
]

HYSTERESIS_COLUMNS = [
    "configuration",
    "rps_per_node",
    "offered_total_rps",
    "up_step",
    "down_step",
    "up_mean_response_time_ms",
    "down_mean_response_time_ms",
    "delta_mean_response_time_ms",
    "up_throughput_rps",
    "down_throughput_rps",
    "delta_throughput_rps",
]


def log(message: str) -> None:
    print(message, file=sys.stderr, flush=True)


def read_schedule(path: Path) -> list[dict]:
    """Load STAIRCASE_STEPS.csv and derive each step's steady-state window."""
    with path.open(newline="") as handle:
        rows = list(csv.DictReader(handle))

    if not rows:
        raise SystemExit(f"ERROR: {path} has no step rows")

    steps = []
    for row in rows:
        start = int(row["start_offset_s"])
        end = int(row["end_offset_s"])
        warmup = int(row["warmup_s"])
        window = end - start - warmup
        if window <= 0:
            raise SystemExit(
                f"ERROR: step {row['step']} has no steady-state window "
                f"(start={start} end={end} warmup={warmup})"
            )
        steps.append(
            {
                "step": int(row["step"]),
                "leg": row["leg"],
                "rps_per_node": row["rps_per_node"],
                "total_rps": int(row["total_rps"]),
                # Inclusive lower bound, exclusive upper bound, both relative to
                # the run's first sample.
                "window_start_s": start + warmup,
                "window_end_s": end,
                "window_s": window,
            }
        )
    return steps


def discover_files(dataset_directory: Path, configurations: list[str]) -> dict[str, Path]:
    found = {}
    for configuration in configurations:
        path = dataset_directory / f"RPS_DATASET_{configuration}_STAIRCASE.csv"
        if path.exists():
            found[configuration] = path
        else:
            log(f"  WARNING: no staircase dataset for {configuration} at {path.name}")
    if not found:
        raise SystemExit(
            f"ERROR: no RPS_DATASET_<CONFIG>_STAIRCASE.csv found in {dataset_directory}"
        )
    return found


def register_steps(connection: duckdb.DuckDBPyConnection, steps: list[dict]) -> None:
    connection.execute(
        """
        CREATE TABLE steps (
            step           INTEGER,
            leg            VARCHAR,
            rps_per_node   VARCHAR,
            total_rps      INTEGER,
            window_start_s BIGINT,
            window_end_s   BIGINT,
            window_s       BIGINT
        )
        """
    )
    connection.executemany(
        "INSERT INTO steps VALUES (?, ?, ?, ?, ?, ?, ?)",
        [
            (
                step["step"],
                step["leg"],
                step["rps_per_node"],
                step["total_rps"],
                step["window_start_s"],
                step["window_end_s"],
                step["window_s"],
            )
            for step in steps
        ],
    )


def read_csv_expression(files: dict[str, Path]) -> str:
    """A read_csv_auto() call over the staircase CSVs, with the two columns this
    script depends on pinned so the sniffer cannot mistype them on a file whose
    head happens to be unrepresentative."""
    file_list = ", ".join(f"'{path}'" for path in files.values())
    return (
        f"read_csv_auto([{file_list}], filename=true, header=true, "
        "types={'timestamp': 'BIGINT', 'metric_value': 'DOUBLE'})"
    )


def find_time_origins(
    connection: duckdb.DuckDBPyConnection, files: dict[str, Path]
) -> None:
    """Step windows are offsets from the start of the run, so each file needs its
    own origin. k6 starts every scenario together, so the first http_reqs sample
    is the run start to within the warmup that gets discarded anyway."""
    log("Scan 1/2: locating each run's first sample...")
    connection.execute(
        f"""
        CREATE TABLE origins AS
        SELECT filename, MIN(timestamp) AS started_at
        FROM {read_csv_expression(files)}
        WHERE metric_name = 'http_reqs'
        GROUP BY filename
        """
    )
    for filename, started_at in connection.execute(
        "SELECT filename, started_at FROM origins ORDER BY filename"
    ).fetchall():
        log(f"  {Path(filename).name}: origin={started_at}")


def aggregate_steps(
    connection: duckdb.DuckDBPyConnection, files: dict[str, Path]
) -> list[tuple]:
    """One scan for every metric, split by step window.

    Conditional aggregates rather than one query per metric: at tens of millions
    of rows per file, the scan dominates and three passes would triple it.
    """
    log("Scan 2/2: aggregating every metric per step window...")
    configuration_pattern = "|".join(CONFIGURATION_TO_LABEL)
    return connection.execute(
        rf"""
        WITH windowed AS (
            SELECT
                regexp_extract(
                    m.filename,
                    'RPS_DATASET_({configuration_pattern})_STAIRCASE\.csv',
                    1
                ) AS configuration,
                s.step,
                m.metric_name,
                m.metric_value
            FROM {read_csv_expression(files)} AS m
            JOIN origins o
              ON o.filename = m.filename
            JOIN steps s
              ON (m.timestamp - o.started_at) >= s.window_start_s
             AND (m.timestamp - o.started_at) <  s.window_end_s
            WHERE m.metric_name IN (
                'http_req_duration', 'http_reqs', 'http_req_failed', 'dropped_iterations'
            )
        )
        SELECT
            configuration,
            step,
            AVG(metric_value) FILTER (WHERE metric_name = 'http_req_duration')
                AS mean_response_time_ms,
            approx_quantile(metric_value, 0.95) FILTER (WHERE metric_name = 'http_req_duration')
                AS p95_response_time_ms,
            approx_quantile(metric_value, 0.99) FILTER (WHERE metric_name = 'http_req_duration')
                AS p99_response_time_ms,
            COUNT(*) FILTER (WHERE metric_name = 'http_reqs')
                AS total_requests,
            100.0 * AVG(metric_value) FILTER (WHERE metric_name = 'http_req_failed')
                AS failure_rate_percent,
            COALESCE(SUM(metric_value) FILTER (WHERE metric_name = 'dropped_iterations'), 0)
                AS dropped_iterations
        FROM windowed
        GROUP BY configuration, step
        ORDER BY configuration, step
        """
    ).fetchall()


def build_evaluation_rows(
    aggregates: list[tuple], steps: list[dict]
) -> list[dict]:
    step_by_number = {step["step"]: step for step in steps}
    rows = []
    for (
        configuration,
        step_number,
        mean_response_time,
        p95,
        p99,
        total_requests,
        failure_rate,
        dropped_iterations,
    ) in aggregates:
        step = step_by_number[step_number]
        rows.append(
            {
                "configuration": configuration,
                "step": step_number,
                "leg": step["leg"],
                "rps_per_node": step["rps_per_node"],
                "offered_total_rps": step["total_rps"],
                "window_s": step["window_s"],
                "mean_response_time_ms": mean_response_time,
                "p95_response_time_ms": p95,
                "p99_response_time_ms": p99,
                # Designed window, not observed span -- see the module docstring.
                "throughput_rps": total_requests / step["window_s"],
                "failure_rate_percent": failure_rate,
                "total_requests": int(total_requests),
                "dropped_iterations": int(dropped_iterations),
            }
        )
    return rows


def build_hysteresis_rows(evaluation_rows: list[dict]) -> list[dict]:
    """Pair each up-leg step with the down-leg step at the same offered load.

    Matching is on the full per-node rate vector, not the total, so two steps
    that happen to sum alike but distribute differently are never paired. The
    peak is visited once and has no partner.
    """
    rows = []
    for configuration in sorted({row["configuration"] for row in evaluation_rows}):
        by_leg: dict[str, dict[str, dict]] = {"up": {}, "down": {}}
        for row in evaluation_rows:
            if row["configuration"] != configuration:
                continue
            if row["leg"] in by_leg:
                by_leg[row["leg"]][row["rps_per_node"]] = row

        for rps_per_node, up in sorted(
            by_leg["up"].items(), key=lambda item: item[1]["offered_total_rps"]
        ):
            down = by_leg["down"].get(rps_per_node)
            if down is None:
                log(
                    f"  WARNING: {configuration} step {up['step']} "
                    f"({rps_per_node}) has no down-leg counterpart"
                )
                continue
            rows.append(
                {
                    "configuration": configuration,
                    "rps_per_node": rps_per_node,
                    "offered_total_rps": up["offered_total_rps"],
                    "up_step": up["step"],
                    "down_step": down["step"],
                    "up_mean_response_time_ms": up["mean_response_time_ms"],
                    "down_mean_response_time_ms": down["mean_response_time_ms"],
                    "delta_mean_response_time_ms": (
                        down["mean_response_time_ms"] - up["mean_response_time_ms"]
                    ),
                    "up_throughput_rps": up["throughput_rps"],
                    "down_throughput_rps": down["throughput_rps"],
                    "delta_throughput_rps": (
                        down["throughput_rps"] - up["throughput_rps"]
                    ),
                }
            )
    return rows


def write_csv(path: Path, columns: list[str], rows: list[dict]) -> None:
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(rows)
    log(f"Wrote {path.name} ({len(rows)} rows)")


def render(value: object, decimals: int = 3) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, float):
        return f"{value:.{decimals}f}"
    return str(value)


def print_evaluation_table(rows: list[dict]) -> None:
    for configuration in sorted({row["configuration"] for row in rows}):
        label = CONFIGURATION_TO_LABEL.get(configuration, configuration)
        print()
        print(f"=== {label} ({configuration}) -- per step, warmup discarded ===")
        header = (
            f"{'St':<3} {'leg':<5} {'req/s per node':<22} {'offered':>8} "
            f"{'mean ms':>10} {'p95 ms':>10} {'p99 ms':>10} "
            f"{'thr req/s':>10} {'fail %':>8} {'dropped':>10}"
        )
        print(header)
        print("-" * len(header))
        for row in sorted(
            (row for row in rows if row["configuration"] == configuration),
            key=lambda row: row["step"],
        ):
            print(
                f"{row['step']:<3} {row['leg']:<5} {row['rps_per_node']:<22} "
                f"{row['offered_total_rps']:>8} "
                f"{render(row['mean_response_time_ms']):>10} "
                f"{render(row['p95_response_time_ms']):>10} "
                f"{render(row['p99_response_time_ms']):>10} "
                f"{render(row['throughput_rps'], 1):>10} "
                f"{render(row['failure_rate_percent']):>8} "
                f"{row['dropped_iterations']:>10}"
            )


def print_hysteresis_table(rows: list[dict]) -> None:
    print()
    print("=== HYSTERESIS -- same offered load, down leg minus up leg ===")
    print("    positive delta ms = still slower on the way down than it was on the way up")
    header = (
        f"{'configuration':<14} {'req/s per node':<22} {'offered':>8} "
        f"{'up ms':>10} {'down ms':>10} {'delta ms':>10} "
        f"{'up thr':>9} {'down thr':>9} {'delta thr':>10}"
    )
    print(header)
    print("-" * len(header))
    for row in rows:
        label = CONFIGURATION_TO_LABEL.get(row["configuration"], row["configuration"])
        print(
            f"{label:<14} {row['rps_per_node']:<22} {row['offered_total_rps']:>8} "
            f"{render(row['up_mean_response_time_ms']):>10} "
            f"{render(row['down_mean_response_time_ms']):>10} "
            f"{render(row['delta_mean_response_time_ms']):>10} "
            f"{render(row['up_throughput_rps'], 1):>9} "
            f"{render(row['down_throughput_rps'], 1):>9} "
            f"{render(row['delta_throughput_rps'], 1):>10}"
        )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Aggregate staircase runs into per-step and hysteresis tables."
    )
    parser.add_argument(
        "--dataset-dir",
        type=Path,
        default=WORKING_DIRECTORY / "dataset",
        help="directory holding RPS_DATASET_<CONFIG>_STAIRCASE.csv (default: ./dataset)",
    )
    parser.add_argument(
        "--schedule",
        type=Path,
        help="STAIRCASE_STEPS.csv written by traffic-suite.sh "
             "(default: <dataset-dir>/STAIRCASE_STEPS.csv)",
    )
    parser.add_argument(
        "--configurations",
        default=",".join(DEFAULT_CONFIGURATIONS),
        help=f"comma separated (default: {','.join(DEFAULT_CONFIGURATIONS)})",
    )
    arguments = parser.parse_args()

    schedule_path = arguments.schedule or arguments.dataset_dir / "STAIRCASE_STEPS.csv"
    if not schedule_path.exists():
        log(f"ERROR: schedule not found at {schedule_path}")
        log("       traffic-suite.sh --mode staircase writes it next to the datasets.")
        return 1

    configurations = [
        token.strip().upper()
        for token in arguments.configurations.split(",")
        if token.strip()
    ]

    log(f"Reading schedule: {schedule_path}")
    steps = read_schedule(schedule_path)
    total_seconds = max(step["window_end_s"] for step in steps)
    log(
        f"  {len(steps)} steps, {total_seconds}s total, "
        f"{steps[0]['window_end_s'] - steps[0]['window_start_s']}s measured per step"
    )

    log(f"Discovering datasets in {arguments.dataset_dir}...")
    files = discover_files(arguments.dataset_dir, configurations)
    for configuration, path in files.items():
        log(f"  {configuration}: {path.name}")

    connection = duckdb.connect()
    register_steps(connection, steps)
    find_time_origins(connection, files)
    aggregates = aggregate_steps(connection, files)
    connection.close()

    if not aggregates:
        log("ERROR: no rows fell inside any step window.")
        log("       Check that the schedule matches the runs (step duration, step count).")
        return 1

    evaluation_rows = build_evaluation_rows(aggregates, steps)
    hysteresis_rows = build_hysteresis_rows(evaluation_rows)

    write_csv(EVALUATION_OUTPUT, EVALUATION_COLUMNS, evaluation_rows)
    write_csv(HYSTERESIS_OUTPUT, HYSTERESIS_COLUMNS, hysteresis_rows)

    print_evaluation_table(evaluation_rows)
    print_hysteresis_table(hysteresis_rows)
    log("Done.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
