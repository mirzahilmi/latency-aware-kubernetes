import sys
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np

# Set style
sns.set_style("whitegrid")
plt.rcParams["figure.figsize"] = (14, 8)

BASELINE_FILE = f"./data/dataset/RPS_DATASET_BASELINE_TESTCASE_{sys.argv[1]}.csv"
SOLUTION_FILE = f"./data/dataset/RPS_DATASET_SOLUTION_TESTCASE_{sys.argv[1]}.csv"


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


def plot_response_time_overtime(baseline_df, solution_df):
    """1. Response time over time - WITHOUT smoothing to show real data"""
    fig, ax = plt.subplots(figsize=(14, 6))

    baseline_rt = get_response_times(baseline_df)
    solution_rt = get_response_times(solution_df)

    # Group by relative_time and calculate mean (no rolling average)
    baseline_grouped = (
        baseline_rt.groupby("relative_time")["metric_value"].mean().reset_index()
    )
    solution_grouped = (
        solution_rt.groupby("relative_time")["metric_value"].mean().reset_index()
    )

    # Plot raw data without rolling average
    ax.plot(
        baseline_grouped["relative_time"],
        baseline_grouped["metric_value"],  # Raw data, no smoothing
        label="Baseline",
        linewidth=1.5,
        alpha=0.8,
    )
    ax.plot(
        solution_grouped["relative_time"],
        solution_grouped["metric_value"],  # Raw data, no smoothing
        label="Solution",
        linewidth=1.5,
        alpha=0.8,
    )

    ax.set_xlabel("Time (seconds)", fontsize=12)
    ax.set_ylabel("Response Time (ms)", fontsize=12)
    ax.set_title("Response Time", fontsize=14, fontweight="bold")
    ax.legend(fontsize=11)
    ax.grid(True, alpha=0.3)

    plt.tight_layout()
    context = sys.argv[2]
    plt.savefig(
        f"visuals/{context}/1_response_time_overtime.png", dpi=300, bbox_inches="tight"
    )
    plt.close()


def plot_rps_overtime(baseline_df, solution_df):
    """2. Requests per second over time - WITHOUT smoothing to show real data"""
    fig, ax = plt.subplots(figsize=(14, 6))

    baseline_rps = calculate_rps(baseline_df)
    solution_rps = calculate_rps(solution_df)

    # Plot raw data without any smoothing
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

    ax.set_xlabel("Time (seconds)", fontsize=12)
    ax.set_ylabel("Requests per Second", fontsize=12)
    ax.set_title("Requests per Second", fontsize=14, fontweight="bold")
    ax.legend(fontsize=11)
    ax.grid(True, alpha=0.3)

    plt.tight_layout()
    context = sys.argv[2]
    plt.savefig(f"visuals/{context}/2_rps_overtime.png", dpi=300, bbox_inches="tight")
    plt.close()


def plot_failed_requests_percentage(baseline_df, solution_df):
    """5. Failed requests percentage as horizontal bar chart"""
    fig, ax = plt.subplots(figsize=(10, 4))

    baseline_failed = get_failed_requests(baseline_df)
    solution_failed = get_failed_requests(solution_df)
    baseline_total = len(get_request_counts(baseline_df))
    solution_total = len(get_request_counts(solution_df))

    baseline_failed_count = baseline_failed["metric_value"].sum()
    solution_failed_count = solution_failed["metric_value"].sum()

    baseline_pct = (
        (baseline_failed_count / baseline_total * 100) if baseline_total > 0 else 0
    )
    solution_pct = (
        (solution_failed_count / solution_total * 100) if solution_total > 0 else 0
    )

    dataset_type = ["Baseline", "Solution"]
    data = [baseline_pct, solution_pct]
    dataset_type.reverse()
    data.reverse()
    colors = ["#d62728" for p in data]

    bars = ax.barh(dataset_type, data, color=colors)

    ax.set_title("Failed Requests (%)", fontsize=12)
    ax.set_xlim(left=0.0)
    ax.spines[["right", "top"]].set_visible(False)
    ax.bar_label(
        bars,
        padding=5,
        color="black",
        fontsize=12,
        label_type="edge",
        fmt="%.2f%%",
        fontweight="bold",
    )

    context = sys.argv[2]

    plt.tight_layout()
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


def plot_response_time_percentiles(baseline_df, solution_df):
    """6. Response time percentiles as bar chart - WITH DEBUG INFO"""
    fig, ax = plt.subplots(figsize=(12, 6))

    baseline_rt = get_response_times(baseline_df)["metric_value"].values
    solution_rt = get_response_times(solution_df)["metric_value"].values

    print(f"\nDEBUG - Percentile Calculation:")
    print(f"Baseline: {len(baseline_rt)} data points")
    print(f"Solution: {len(solution_rt)} data points")
    print(f"Baseline min/max: {baseline_rt.min():.2f} / {baseline_rt.max():.2f} ms")
    print(f"Solution min/max: {solution_rt.min():.2f} / {solution_rt.max():.2f} ms")

    percentiles = [50, 75, 90, 95]
    x = np.arange(len(percentiles))
    width = 0.35

    baseline_pct = np.percentile(baseline_rt, percentiles)
    solution_pct = np.percentile(solution_rt, percentiles)

    print("\nPercentile values:")
    for i, p in enumerate(percentiles):
        print(
            f"P{p}: Baseline={baseline_pct[i]:.2f}ms, Solution={solution_pct[i]:.2f}ms"
        )

    bars1 = ax.bar(x - width / 2, baseline_pct, width, label="Baseline", alpha=0.8)
    bars2 = ax.bar(x + width / 2, solution_pct, width, label="Solution", alpha=0.8)

    # Add value labels on bars
    for bars in [bars1, bars2]:
        for bar in bars:
            height = bar.get_height()
            ax.text(
                bar.get_x() + bar.get_width() / 2.0,
                height,
                f"{height:.1f}",
                ha="center",
                va="bottom",
                fontsize=9,
            )

    ax.set_xlabel("Percentile", fontsize=12)
    ax.set_ylabel("Response Time (ms)", fontsize=12)
    ax.set_title("Response Time", fontsize=14, fontweight="bold")
    ax.set_xticks(x)
    ax.set_xticklabels([f"P{p}" for p in percentiles])
    ax.legend(fontsize=11)
    ax.grid(True, alpha=0.3, axis="y")

    plt.tight_layout()
    context = sys.argv[2]
    plt.savefig(
        f"visuals/{context}/4_response_time_percentiles.png",
        dpi=300,
        bbox_inches="tight",
    )
    plt.close()


def plot_response_time_boxplot(baseline_df, solution_df):
    """7. Response time box plot"""
    fig, ax = plt.subplots(figsize=(10, 6))

    baseline_rt = get_response_times(baseline_df)["metric_value"].values
    solution_rt = get_response_times(solution_df)["metric_value"].values

    data = [baseline_rt, solution_rt]
    labels = ["Baseline", "Solution"]

    ax.boxplot(data, tick_labels=labels)
    ax.set_title("Response Time (ms)")

    plt.tight_layout()
    context = sys.argv[2]
    plt.savefig(
        f"visuals/{context}/5_response_time_boxplot.png", dpi=300, bbox_inches="tight"
    )
    plt.close()


def plot_rps_boxplot(baseline_df, solution_df):
    """8. Requests per second box plot"""
    fig, ax = plt.subplots(figsize=(10, 6))

    baseline_rps = calculate_rps(baseline_df)["metric_value"].values
    solution_rps = calculate_rps(solution_df)["metric_value"].values

    data = [baseline_rps, solution_rps]
    labels = ["Baseline", "Solution"]

    ax.boxplot(data, tick_labels=labels)
    ax.set_title("Requests per Second")

    # plt.tight_layout()
    context = sys.argv[2]
    plt.savefig(f"visuals/{context}/6_rps_boxplot.png", dpi=300, bbox_inches="tight")
    plt.close()


def print_summary_statistics(baseline_df, solution_df):
    """Print summary statistics"""
    print("\n" + "=" * 80)
    print("K6 PERFORMANCE ANALYSIS SUMMARY")
    print("=" * 80)

    # Response Time Statistics
    baseline_rt = get_response_times(baseline_df)["metric_value"].values
    solution_rt = get_response_times(solution_df)["metric_value"].values

    print("\n--- RESPONSE TIME (ms) ---")
    print(f"{'Metric':<20} {'Baseline':>15} {'Solution':>15} {'Improvement':>15}")
    print("-" * 68)

    metrics = {
        "Mean": (np.mean(baseline_rt), np.mean(solution_rt)),
        "Median": (np.median(baseline_rt), np.median(solution_rt)),
        "P95": (np.percentile(baseline_rt, 95), np.percentile(solution_rt, 95)),
        "P99": (np.percentile(baseline_rt, 99), np.percentile(solution_rt, 99)),
        "Min": (np.min(baseline_rt), np.min(solution_rt)),
        "Max": (np.max(baseline_rt), np.max(solution_rt)),
        "Std Dev": (np.std(baseline_rt), np.std(solution_rt)),
    }

    for metric, (base, sol) in metrics.items():
        improvement = ((base - sol) / base * 100) if base != 0 else 0
        print(f"{metric:<20} {base:>15.2f} {sol:>15.2f} {improvement:>14.2f}%")

    # RPS Statistics
    baseline_rps = calculate_rps(baseline_df)["metric_value"].values
    solution_rps = calculate_rps(solution_df)["metric_value"].values

    print("\n--- REQUESTS PER SECOND ---")
    print(f"{'Metric':<20} {'Baseline':>15} {'Solution':>15} {'Change':>15}")
    print("-" * 68)

    rps_metrics = {
        "Mean": (np.mean(baseline_rps), np.mean(solution_rps)),
        "Median": (np.median(baseline_rps), np.median(solution_rps)),
        "Min": (np.min(baseline_rps), np.min(solution_rps)),
        "Max": (np.max(baseline_rps), np.max(solution_rps)),
    }

    for metric, (base, sol) in rps_metrics.items():
        change = ((sol - base) / base * 100) if base != 0 else 0
        print(f"{metric:<20} {base:>15.2f} {sol:>15.2f} {change:>14.2f}%")

    # Failed Requests
    baseline_failed = get_failed_requests(baseline_df)["metric_value"].sum()
    solution_failed = get_failed_requests(solution_df)["metric_value"].sum()
    baseline_total = len(get_request_counts(baseline_df))
    solution_total = len(get_request_counts(solution_df))

    print("\n--- FAILED REQUESTS ---")
    print(f"{'Dataset':<20} {'Total Requests':>15} {'Failed':>15} {'Failure Rate':>15}")
    print("-" * 68)
    print(
        f"{'Baseline':<20} {baseline_total:>15} {int(baseline_failed):>15} "
        f"{(baseline_failed / baseline_total * 100):>14.2f}%"
    )
    print(
        f"{'Solution':<20} {solution_total:>15} {int(solution_failed):>15} "
        f"{(solution_failed / solution_total * 100):>14.2f}%"
    )

    print("\n" + "=" * 80)


def main():
    print("Loading k6 performance data...")
    print(f"Baseline file: {BASELINE_FILE}")
    print(f"Solution file: {SOLUTION_FILE}")

    # Load data
    baseline_df = load_and_clean_data(BASELINE_FILE, "Baseline")
    solution_df = load_and_clean_data(SOLUTION_FILE, "Solution")

    print(f"\nBaseline: {len(baseline_df)} records after removing warmup")
    print(f"Solution: {len(solution_df)} records after removing warmup")

    # Generate all visualizations
    print("\nGenerating visualizations...")

    print("  1. Response time over time...")
    plot_response_time_overtime(baseline_df, solution_df)

    print("  2. Requests per second over time...")
    plot_rps_overtime(baseline_df, solution_df)

    print("  3. Failed requests percentage...")
    plot_failed_requests_percentage(baseline_df, solution_df)

    print("  4. Response time percentiles...")
    plot_response_time_percentiles(baseline_df, solution_df)

    print("  5. Response time box plot...")
    plot_response_time_boxplot(baseline_df, solution_df)

    print("  6. Requests per second box plot...")
    plot_rps_boxplot(baseline_df, solution_df)

    # Print summary statistics
    print_summary_statistics(baseline_df, solution_df)


if __name__ == "__main__":
    main()
