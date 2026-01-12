import sys
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np
import matplotlib.ticker as mtick

# Set style
sns.set_style("whitegrid")
plt.rcParams["figure.figsize"] = (14, 8)

BASELINE_FILE = f"./data/dataset/RPS_DATASET_BASELINE_TESTCASE_{sys.argv[1]}.csv"
SOLUTION_FILE = f"./data/dataset/RPS_DATASET_SOLUTION_TESTCASE_{sys.argv[1]}.csv"
IPVS_LC_FILE = f"./data/dataset/RPS_DATASET_IPVS_LC_TESTCASE_{sys.argv[1]}.csv"


def load_and_clean_data(filepath, dataset_name):
    """Load k6 CSV data and remove warmup period (first 60 seconds) and trim after 7 minutes"""
    df = pd.read_csv(filepath)

    # Get the minimum timestamp
    min_timestamp = df["timestamp"].min()

    # Remove first 60 seconds (warmup period) and keep only 7 minutes total (420 seconds)
    # So we keep data from min_timestamp+60 to min_timestamp+420
    df = df[
        (df["timestamp"] >= (min_timestamp + 60))
        & (df["timestamp"] <= (min_timestamp + 420))
    ].copy()

    # Add dataset identifier
    df["dataset"] = dataset_name

    # Convert timestamp to relative seconds for easier plotting
    df["relative_time"] = df["timestamp"] - df["timestamp"].min()

    return df


def get_response_times(df):
    """Extract response time data (http_req_duration) - FIXED filtering"""
    # Filter for http_req_duration AND expected_response == True
    mask = (df["metric_name"] == "http_req_duration") & (
        df["expected_response"] == True
    )
    return df[mask].copy()


def get_request_counts(df):
    """Extract request count data (http_reqs)"""
    return df[df["metric_name"] == "http_reqs"].copy()


def get_failed_requests(df):
    """Extract failed request data (http_req_failed)"""
    return df[df["metric_name"] == "http_req_failed"].copy()


def calculate_rps(df, window=1):
    """Calculate requests per second"""
    req_data = get_request_counts(df)

    # Group by second and count requests
    rps = (
        req_data.groupby(req_data["timestamp"].astype(int))
        .agg({"metric_value": "sum", "relative_time": "first", "dataset": "first"})
        .reset_index(drop=True)
    )

    return rps


def get_title():
    match sys.argv[1]:
        case "1":
            return (
                "Concentrated Requests",
                "Worker 1 (200 RPS) - Worker 2 (0 RPS) - Worker 3 (0 RPS) - Worker 4 (0 RPS)",
            )
        case "2":
            return (
                "Concentrated Requests",
                "Worker 1 (1600 RPS) - Worker 2 (0 RPS) - Worker 3 (0 RPS) - Worker 4 (0 RPS)",
            )
        case "3":
            return (
                "Distributed Requests (Balance)",
                "Worker 1 (200 RPS) - Worker 2 (200 RPS) - Worker 3 (200 RPS) - Worker 4 (200 RPS)",
            )
        case "4":
            return (
                "Distributed Requests (Balance)",
                "Worker 1 (800 RPS) - Worker 2 (800 RPS) - Worker 3 (800 RPS) - Worker 4 (800 RPS)",
            )
        case "5":
            return (
                "Distributed Requests (Imbalance)",
                "Worker 1 (800 RPS) - Worker 2 (400 RPS) - Worker 3 (400 RPS) - Worker 4 (400 RPS)",
            )
        case "6":
            return (
                "Distributed Requests (Imbalance)",
                "Worker 1 (3200 RPS) - Worker 2 (400 RPS) - Worker 3 (400 RPS) - Worker 4 (400 RPS)",
            )
        case "7":
            return (
                "Distributed Requests (Imbalance)",
                "Worker 1 (1600 RPS) - Worker 2 (800 RPS) - Worker 3 (800 RPS) - Worker 4 (400 RPS)",
            )
        case "8":
            return (
                "Distributed Requests (Imbalance)",
                "Worker 1 (1200 RPS) - Worker 2 (800 RPS) - Worker 3 (800 RPS) - Worker 4 (800 RPS)",
            )
        case _:
            return ("Unknown", "N/A")


def plot_response_time_overtime(baseline_df, solution_df, ipvs_lc_df):
    fig, ax = plt.subplots(figsize=(14, 6))

    baseline_rt = get_response_times(baseline_df)
    solution_rt = get_response_times(solution_df)
    ipvs_lc_rt = get_response_times(ipvs_lc_df)

    baseline_grouped = (
        baseline_rt.groupby("relative_time")["metric_value"].mean().reset_index()
    )
    solution_grouped = (
        solution_rt.groupby("relative_time")["metric_value"].mean().reset_index()
    )
    ipvs_lc_grouped = (
        ipvs_lc_rt.groupby("relative_time")["metric_value"].mean().reset_index()
    )

    ax.plot(
        baseline_grouped["relative_time"],
        baseline_grouped["metric_value"],
        label="Baseline",
        linewidth=1.5,
        alpha=0.8,
    )
    ax.plot(
        solution_grouped["relative_time"],
        solution_grouped["metric_value"],
        label="Solution",
        linewidth=1.5,
        alpha=0.8,
    )
    ax.plot(
        ipvs_lc_grouped["relative_time"],
        ipvs_lc_grouped["metric_value"],
        label="Least Connection",
        linewidth=1.5,
        alpha=0.8,
    )

    title, subtitle = get_title()
    ax.set_title(title, y=1.11)
    ax.text(
        0.512,
        1.085,
        subtitle,
        ha="center",
        fontsize=9,
        color="#666",
        transform=ax.transAxes,
    )
    ax.set_xlabel("Testcase Duration (seconds)", labelpad=10)
    ax.set_ylabel("Response Time (milliseconds)")
    ax.legend(loc="upper center", bbox_to_anchor=(0.5, 1.08), ncol=5, frameon=False)
    ax.grid(True, alpha=0.3)
    plt.tight_layout()

    context = sys.argv[2]
    plt.savefig(
        f"visuals/{context}/1_response_time_overtime.png", dpi=300, bbox_inches="tight"
    )
    plt.close()


def plot_rps_overtime(baseline_df, solution_df, ipvs_lc_df):
    fig, ax = plt.subplots(figsize=(14, 6))

    baseline_rps = calculate_rps(baseline_df)
    solution_rps = calculate_rps(solution_df)
    ipvs_lc_rps = calculate_rps(ipvs_lc_df)

    ax.plot(
        baseline_rps["relative_time"],
        baseline_rps["metric_value"],
        label="Baseline",
        linewidth=1.5,
        alpha=0.8,
    )
    ax.plot(
        solution_rps["relative_time"],
        solution_rps["metric_value"],
        label="Solution",
        linewidth=1.5,
        alpha=0.8,
    )
    ax.plot(
        ipvs_lc_rps["relative_time"],
        ipvs_lc_rps["metric_value"],
        label="Least Connection",
        linewidth=1.5,
        alpha=0.8,
    )

    title, subtitle = get_title()
    ax.set_title(title, y=1.11)
    ax.text(
        0.512,
        1.085,
        subtitle,
        ha="center",
        fontsize=9,
        color="#666",
        transform=ax.transAxes,
    )
    ax.set_xlabel("Testcase Duration (seconds)", labelpad=10)
    ax.set_ylabel("Requests per Second")
    ax.legend(loc="upper center", bbox_to_anchor=(0.5, 1.08), ncol=5, frameon=False)
    ax.grid(True, alpha=0.3)
    plt.tight_layout()

    plt.tight_layout()
    context = sys.argv[2]
    plt.savefig(f"visuals/{context}/2_rps_overtime.png", dpi=300, bbox_inches="tight")
    plt.close()


def plot_failed_requests_percentage(baseline_df, solution_df, ipvs_lc_df):
    fig, ax = plt.subplots(figsize=(10, 4))

    baseline_failed = get_failed_requests(baseline_df)
    solution_failed = get_failed_requests(solution_df)
    ipvs_lc_failed = get_failed_requests(ipvs_lc_df)

    baseline_total = len(get_request_counts(baseline_df))
    solution_total = len(get_request_counts(solution_df))
    ipvs_lc_total = len(get_request_counts(ipvs_lc_df))

    baseline_failed_count = baseline_failed["metric_value"].sum()
    solution_failed_count = solution_failed["metric_value"].sum()
    ipvs_lc_failed_count = ipvs_lc_failed["metric_value"].sum()

    baseline_pct = (
        (baseline_failed_count / baseline_total * 100) if baseline_total > 0 else 0
    )
    solution_pct = (
        (solution_failed_count / solution_total * 100) if solution_total > 0 else 0
    )
    ipvs_lc_pct = (
        (ipvs_lc_failed_count / ipvs_lc_total * 100) if solution_total > 0 else 0
    )

    dataset_type = ["Baseline", "Least Connection", "Solution"]
    data = [baseline_pct, ipvs_lc_pct, solution_pct]
    colors = ["tab:blue", "tab:green", "tab:orange"]

    bars = ax.barh(dataset_type, data, color=colors, label=dataset_type)
    ax.bar_label(
        bars,
        padding=5,
        label_type="edge",
        fmt="%.2f%%",
    )
    ax.invert_yaxis()
    ax.xaxis.set_major_formatter(mtick.PercentFormatter())
    ax.set_xlim(left=0.0)
    ax.spines[["right", "top"]].set_visible(False)
    ax.set_yticks([])
    title, subtitle = get_title()
    ax.set_title(title, y=1.13)
    ax.text(
        0.512,
        1.090,
        subtitle,
        ha="center",
        fontsize=9,
        color="#666",
        transform=ax.transAxes,
    )
    ax.grid(True, alpha=0.3)
    ax.set_xlabel("Failed Requests (%)", labelpad=10)
    ax.legend(loc="upper center", bbox_to_anchor=(0.5, 1.08), ncol=5, frameon=False)
    plt.tight_layout()

    context = sys.argv[2]
    plt.savefig(
        f"visuals/{context}/3_failed_requests_percentage.png",
        dpi=300,
        bbox_inches="tight",
    )
    plt.close()

    data_table = {
        "Scenario": ["Baseline", "Solution"],
        "Total Requests": [baseline_total, solution_total],
        "Total Failed Requests": [baseline_failed_count, solution_failed_count],
    }
    df = pd.DataFrame(data_table)
    df.to_csv(f"visuals/{context}/failed_requests.csv", index=False)


def plot_response_time_percentiles(baseline_df, solution_df, ipvs_lc_df):
    fig, ax = plt.subplots(figsize=(12, 6))

    baseline_rt = get_response_times(baseline_df)["metric_value"].values
    solution_rt = get_response_times(solution_df)["metric_value"].values
    ipvs_lc_rt = get_response_times(ipvs_lc_df)["metric_value"].values

    percentiles = [50, 75, 90, 95]
    x = np.arange(len(percentiles))
    width = 0.25

    baseline_pct = np.percentile(baseline_rt, percentiles)
    solution_pct = np.percentile(solution_rt, percentiles)
    ipvs_lc_pct = np.percentile(ipvs_lc_rt, percentiles)
    colors = ["tab:blue", "tab:green", "tab:orange"]

    bars1 = ax.bar(x - width, baseline_pct, width, label="Baseline", color=colors[0])
    bars2 = ax.bar(x, ipvs_lc_pct, width, label="Least Connection", color=colors[1])
    bars3 = ax.bar(x + width, solution_pct, width, label="Solution", color=colors[2])

    # add labels
    for bars in [bars1, bars2, bars3]:
        for bar in bars:
            h = bar.get_height()
            ax.text(
                bar.get_x() + bar.get_width() / 2,
                h,
                f"{h:.1f}",
                ha="center",
                va="bottom",
                fontsize=9,
            )

    title, subtitle = get_title()
    ax.set_title(title, y=1.11)
    ax.text(
        0.512,
        1.090,
        subtitle,
        ha="center",
        fontsize=9,
        color="#666",
        transform=ax.transAxes,
    )
    ax.set_ylabel("Response Time (milliseconds)")
    ax.set_xticks(x)
    ax.set_xticklabels([f"P{p}" for p in percentiles])
    ax.legend(loc="upper center", bbox_to_anchor=(0.5, 1.08), ncol=5, frameon=False)
    ax.grid(True, alpha=0.3)
    plt.tight_layout()

    context = sys.argv[2]
    plt.savefig(
        f"visuals/{context}/4_response_time_percentiles.png",
        dpi=300,
        bbox_inches="tight",
    )
    plt.close()


def plot_response_time_boxplot(baseline_df, solution_df, ipvs_lc_df):
    """7. Response time box plot"""
    fig, ax = plt.subplots(figsize=(10, 6))

    baseline_rt = get_response_times(baseline_df)["metric_value"].values
    solution_rt = get_response_times(solution_df)["metric_value"].values
    ipvs_lc_rt = get_response_times(ipvs_lc_df)["metric_value"].values

    baseline_low, baseline_high = np.percentile(baseline_rt, [1, 99])
    solution_low, solution_high = np.percentile(solution_rt, [1, 99])
    ipvs_lc_low, ipvs_lc_high = np.percentile(ipvs_lc_rt, [1, 99])

    baseline_filtered = baseline_rt[
        (baseline_rt >= baseline_low) & (baseline_rt <= baseline_high)
    ]
    solution_filtered = solution_rt[
        (solution_rt >= solution_low) & (solution_rt <= solution_high)
    ]
    ipvs_lc_filtered = ipvs_lc_rt[
        (ipvs_lc_rt >= ipvs_lc_low) & (ipvs_lc_rt <= ipvs_lc_high)
    ]

    data = [baseline_filtered, ipvs_lc_filtered, solution_filtered]
    labels = ["Baseline", "Least Connection", "Solution"]
    ax.boxplot(data, tick_labels=labels)

    title, subtitle = get_title()
    ax.set_title(title, y=1.06)
    ax.text(
        0.512,
        1.03,
        subtitle,
        ha="center",
        fontsize=9,
        color="#666",
        transform=ax.transAxes,
    )
    ax.set_ylabel("Response Time (milliseconds)")
    ax.grid(True, alpha=0.3)
    plt.tight_layout()

    context = sys.argv[2]
    plt.savefig(
        f"visuals/{context}/5_response_time_boxplot.png", dpi=300, bbox_inches="tight"
    )
    plt.close()


def plot_rps_boxplot(baseline_df, solution_df, ipvs_lc_df):
    fig, ax = plt.subplots(figsize=(10, 6))

    baseline_rps = calculate_rps(baseline_df)["metric_value"].values
    solution_rps = calculate_rps(solution_df)["metric_value"].values
    ipvs_lc_rps = calculate_rps(ipvs_lc_df)["metric_value"].values

    data = [baseline_rps, ipvs_lc_rps, solution_rps]
    labels = ["Baseline", "Least Connection", "Solution"]
    ax.boxplot(data, tick_labels=labels)

    title, subtitle = get_title()
    ax.set_title(title, y=1.06)
    ax.text(
        0.512,
        1.03,
        subtitle,
        ha="center",
        fontsize=9,
        color="#666",
        transform=ax.transAxes,
    )
    ax.set_ylabel("Request per Second")
    ax.grid(True, alpha=0.3)
    plt.tight_layout()

    context = sys.argv[2]
    plt.savefig(f"visuals/{context}/6_rps_boxplot.png", dpi=300, bbox_inches="tight")
    plt.close()


def main():
    print("Loading k6 performance data...")
    baseline_df = load_and_clean_data(BASELINE_FILE, "Baseline")
    solution_df = load_and_clean_data(SOLUTION_FILE, "Solution")
    ipvs_lc_df = load_and_clean_data(IPVS_LC_FILE, "Least Connection")

    print(f"\nBaseline: {len(baseline_df)} records after removing warmup")
    print(f"Solution: {len(solution_df)} records after removing warmup")
    print(f"Solution: {len(ipvs_lc_df)} records after removing warmup")

    # Generate all visualizations
    print("\nGenerating visualizations...")

    print("  1. Response time over time...")
    plot_response_time_overtime(baseline_df, solution_df, ipvs_lc_df)

    print("  2. Requests per second over time...")
    plot_rps_overtime(baseline_df, solution_df, ipvs_lc_df)

    print("  3. Failed requests percentage...")
    plot_failed_requests_percentage(baseline_df, solution_df, ipvs_lc_df)

    print("  4. Response time percentiles...")
    plot_response_time_percentiles(baseline_df, solution_df, ipvs_lc_df)

    print("  5. Response time box plot...")
    plot_response_time_boxplot(baseline_df, solution_df, ipvs_lc_df)

    print("  6. Requests per second box plot...")
    plot_rps_boxplot(baseline_df, solution_df, ipvs_lc_df)


if __name__ == "__main__":
    main()
