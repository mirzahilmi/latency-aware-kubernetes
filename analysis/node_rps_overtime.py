"""Plot per-worker-node HTTP request throughput over the span of a scenario run.

Pulls `hellopod_requests_served_total` from Prometheus, folds the 16 pod series
into one line per worker node, and renders a line chart over scenario elapsed
time. Worker node identity comes from kubectl so ordering is stable across runs.

Usage:
    uv run node_rps_overtime.py --start 2026-07-29T11:11+07:00 --duration 20m
    uv run node_rps_overtime.py --start ... --end ... --trim --csv out.csv
"""

import argparse
import json
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timedelta

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

METRIC = "hellopod_requests_served_total"

# Validated categorical slots 1,2,3,7 (blue/orange/aqua/violet) — this 4-set
# clears CVD separation and the normal-vision floor on the all-pairs list, which
# is the right gate for overlapping lines. Slot 4 (yellow) is deliberately
# skipped: yellow beside orange fails the normal-vision floor (dE 13.7 < 15).
SERIES_COLORS = ["#2a78d6", "#eb6834", "#1baf7a", "#4a3aa7"]

TEXT_PRIMARY = "#0b0b0b"
TEXT_SECONDARY = "#52514e"
TEXT_MUTED = "#8a8880"
SURFACE = "#fcfcfb"


def parse_duration(text):
    """Parse a Prometheus-style duration ('90s', '20m', '2h') into a timedelta."""
    units = {"s": "seconds", "m": "minutes", "h": "hours"}
    unit = text[-1]
    if unit not in units:
        raise argparse.ArgumentTypeError(
            f"duration {text!r} must end in one of {''.join(units)}"
        )
    try:
        value = float(text[:-1])
    except ValueError:
        raise argparse.ArgumentTypeError(f"duration {text!r} has a non-numeric value")
    return timedelta(**{units[unit]: value})


def fetch_worker_nodes():
    """Return worker node hostnames from kubectl, sorted for stable line ordering.

    Falls back to the node label values Prometheus knows about when kubectl is
    unavailable (e.g. running the analysis away from the cluster).
    """
    try:
        raw = subprocess.run(
            [
                "kubectl",
                "get",
                "nodes",
                "-l",
                "node-role.kubernetes.io/worker",
                "-o",
                "json",
            ],
            capture_output=True,
            text=True,
            check=True,
            timeout=30,
        ).stdout
    except (
        subprocess.CalledProcessError,
        subprocess.TimeoutExpired,
        FileNotFoundError,
    ) as err:
        print(f"  kubectl unavailable ({err}); deriving nodes from Prometheus", file=sys.stderr)
        return None

    return sorted(item["metadata"]["name"] for item in json.loads(raw)["items"])


def prom_query(base_url, path, params):
    url = f"http://{base_url}/api/v1/{path}?{urllib.parse.urlencode(params)}"
    try:
        with urllib.request.urlopen(url, timeout=60) as resp:
            payload = json.load(resp)
    except urllib.error.URLError as err:
        sys.exit(f"error: cannot reach Prometheus at {base_url}: {err}")

    if payload.get("status") != "success":
        sys.exit(f"error: Prometheus rejected the query: {payload.get('error')}")
    return payload["data"]


def detect_scrape_interval(base_url, namespace):
    """Return the scrape interval (seconds) of the job exporting our metric.

    The rate() window has to span at least two scrapes or the query yields
    nothing, and this cluster scrapes hellopod at 1m -- far coarser than the
    usual 15s, so guessing is not safe.
    """
    data = prom_query(base_url, "targets", {"state": "active"})
    for target in data.get("activeTargets", []):
        labels = target.get("labels", {})
        if labels.get("namespace") == namespace or "hellopod" in target.get("scrapePool", ""):
            interval = target.get("scrapeInterval")
            if interval:
                return parse_duration(interval).total_seconds()
    return None


def fetch_node_rps(base_url, namespace, start, end, step, rate_window):
    query = (
        f"sum by (node) (rate({METRIC}{{namespace='{namespace}'}}[{rate_window}]))"
    )
    data = prom_query(
        base_url,
        "query_range",
        {
            "query": query,
            "start": start.isoformat(),
            "end": end.isoformat(),
            "step": f"{int(step)}s",
        },
    )

    frames = []
    for series in data["result"]:
        node = series["metric"]["node"]
        frame = pd.DataFrame(series["values"], columns=["timestamp", "rps"])
        frame["timestamp"] = pd.to_datetime(
            frame["timestamp"].astype(float), unit="s", utc=True
        )
        frame["rps"] = frame["rps"].astype(float)
        frame["node"] = node
        frames.append(frame)

    if not frames:
        sys.exit(
            "error: query returned no series. Check the time window, and that "
            f"--rate-window ({rate_window}) spans at least two scrape intervals."
        )
    return pd.concat(frames, ignore_index=True)


def trim_to_active(df, floor_ratio=0.05):
    """Crop leading/trailing idle samples so the chart spans just the load period.

    Between runs the pods still serve a trickle of probe traffic, which otherwise
    pads the chart with a long flat tail.
    """
    total = df.groupby("timestamp")["rps"].sum()
    threshold = total.max() * floor_ratio
    active = total[total > threshold]
    if active.empty:
        return df
    return df[(df["timestamp"] >= active.index.min()) & (df["timestamp"] <= active.index.max())]


def build_wide(df, nodes):
    """Pivot to one column per node, ordered, with an elapsed-seconds index."""
    wide = df.pivot_table(index="timestamp", columns="node", values="rps").sort_index()

    known = [n for n in nodes if n in wide.columns] if nodes else []
    extra = sorted(c for c in wide.columns if c not in known)
    if extra and nodes:
        print(f"  note: series present in Prometheus but not in kubectl: {extra}")
    wide = wide[known + extra]

    wide.index = (wide.index - wide.index[0]).total_seconds()
    wide.index.name = "elapsed_seconds"
    return wide


def plot(wide, labels, scenario, started_at, out_path):
    sns.set_style("whitegrid")
    fig, ax = plt.subplots(figsize=(14, 14 / 1.62), facecolor=SURFACE)
    ax.set_facecolor(SURFACE)

    for i, node in enumerate(wide.columns):
        color = SERIES_COLORS[i % len(SERIES_COLORS)]
        ax.plot(
            wide.index,
            wide[node],
            label=labels[node],
            color=color,
            linewidth=2,
            solid_capstyle="round",
        )

        # Direct end-of-line labels: identity is never carried by color alone,
        # and they satisfy the relief rule for the low-contrast aqua slot.
        tail = wide[node].dropna()
        if not tail.empty:
            ax.annotate(
                labels[node],
                xy=(tail.index[-1], tail.iloc[-1]),
                xytext=(6, 0),
                textcoords="offset points",
                va="center",
                fontsize=9,
                color=TEXT_SECONDARY,
            )

    ax.set_title(
        f"Per-Node Request Throughput — {scenario} Scenario",
        y=1.09,
        color=TEXT_PRIMARY,
        fontsize=14,
    )
    ax.text(
        0.5,
        1.055,
        f"{METRIC} by worker node · run started {started_at:%Y-%m-%d %H:%M %Z}",
        ha="center",
        fontsize=9,
        color=TEXT_MUTED,
        transform=ax.transAxes,
    )
    ax.set_xlabel("Scenario Elapsed Time (seconds)", labelpad=10, color=TEXT_SECONDARY)
    ax.set_ylabel("Requests per Second", labelpad=10, color=TEXT_SECONDARY)
    ax.legend(
        loc="upper center",
        bbox_to_anchor=(0.5, 1.04),
        ncol=len(wide.columns),
        frameon=False,
    )
    ax.grid(True, alpha=0.25)
    ax.set_ylim(bottom=0)
    ax.margins(x=0.06)
    ax.tick_params(colors=TEXT_SECONDARY)
    for spine in ax.spines.values():
        spine.set_visible(False)

    fig.tight_layout()
    fig.savefig(out_path, dpi=300, bbox_inches="tight", facecolor=SURFACE)
    plt.close(fig)


def summarize(wide, labels):
    """Print the table view of the same data backing the chart."""
    step = wide.index.to_series().diff().median()
    mean = wide.mean()
    share = mean / mean.sum() * 100

    print(f"\n  {'node':<28} {'label':<10} {'mean RPS':>9} {'peak RPS':>9} {'share':>7}")
    print(f"  {'-' * 28} {'-' * 10} {'-' * 9} {'-' * 9} {'-' * 7}")
    for node in wide.columns:
        print(
            f"  {node:<28} {labels[node]:<10} {mean[node]:>9.1f} "
            f"{wide[node].max():>9.1f} {share[node]:>6.1f}%"
        )
    print(f"  {'-' * 28} {'-' * 10} {'-' * 9} {'-' * 9} {'-' * 7}")
    print(
        f"  {'TOTAL':<28} {'':<10} {mean.sum():>9.1f} "
        f"{wide.sum(axis=1).max():>9.1f} {100.0:>6.1f}%"
    )
    print(f"\n  window: {wide.index[-1]:.0f}s across {len(wide)} samples (step {step:.0f}s)")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--start",
        required=True,
        help="scenario start, ISO 8601 with offset (e.g. 2026-07-29T11:11+07:00)",
    )
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--end", help="scenario end, ISO 8601 with offset")
    group.add_argument("--duration", type=parse_duration, help="window length, e.g. 20m")

    parser.add_argument("--prom", default="10.34.211.154:9090", help="Prometheus host:port")
    parser.add_argument("--namespace", default="riset", help="pod namespace")
    parser.add_argument("--step", type=parse_duration, help="sample step (default: scrape interval)")
    parser.add_argument(
        "--rate-window",
        help="rate() lookback (default: 2x scrape interval, the minimum that yields data)",
    )
    parser.add_argument("--scenario", default="800/800/800/800", help="scenario name for the title")
    parser.add_argument("--trim", action="store_true", help="crop idle samples to the load period")
    parser.add_argument("--out", default="visuals/node_rps_overtime.png", help="output PNG path")
    parser.add_argument("--csv", help="also write the plotted series to this CSV")
    args = parser.parse_args()

    start = datetime.fromisoformat(args.start)
    end = datetime.fromisoformat(args.end) if args.end else start + args.duration

    print(f"Querying {args.prom} for {start:%H:%M} -> {end:%H:%M} ({end - start})")

    interval = detect_scrape_interval(args.prom, args.namespace)
    if interval is None:
        interval = 60.0
        print("  could not detect scrape interval; assuming 60s")
    else:
        print(f"  scrape interval: {interval:.0f}s")

    step = args.step.total_seconds() if args.step else interval
    # rate() needs >=2 samples in its window, so never go below 2x the interval.
    rate_window = args.rate_window or f"{int(interval * 2)}s"

    nodes = fetch_worker_nodes()
    if nodes:
        print(f"  worker nodes: {', '.join(nodes)}")

    df = fetch_node_rps(args.prom, args.namespace, start, end, step, rate_window)
    if args.trim:
        before = df["timestamp"].nunique()
        df = trim_to_active(df)
        print(f"  trimmed idle samples: {before} -> {df['timestamp'].nunique()}")

    wide = build_wide(df, nodes)
    labels = {node: f"Worker {i + 1}" for i, node in enumerate(wide.columns)}

    summarize(wide, labels)

    out = args.out
    plot(wide, labels, args.scenario, start, out)
    print(f"\n  wrote {out}")

    if args.csv:
        wide.rename(columns=labels).to_csv(args.csv)
        print(f"  wrote {args.csv}")


if __name__ == "__main__":
    main()
