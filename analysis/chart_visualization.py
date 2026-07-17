import sys
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns

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
        case "4":
            return (
                "Distributed Requests (Balance)",
                "Worker 1 (800 RPS) - Worker 2 (800 RPS) - Worker 3 (800 RPS) - Worker 4 (800 RPS)",
            )
        case "6":
            return (
                "Distributed Requests (Imbalance)",
                "Worker 1 (3200 RPS) - Worker 2 (400 RPS) - Worker 3 (400 RPS) - Worker 4 (400 RPS)",
            )
        case _:
            return ("Unknown", "N/A")


def plot_response_time_overtime(baseline_df, solution_df, ipvs_lc_df):
    fig, ax = plt.subplots(figsize=(14, 14 / 1.62))

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
        label="BASELINE",
        linewidth=1.5,
        alpha=0.8,
    )
    ax.plot(
        solution_grouped["relative_time"],
        solution_grouped["metric_value"],
        label="EWMA",
        linewidth=1.5,
        alpha=0.8,
    )
    ax.plot(
        ipvs_lc_grouped["relative_time"],
        ipvs_lc_grouped["metric_value"],
        label="LEASTCONN",
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
    fig, ax = plt.subplots(figsize=(14, 14 / 1.62))

    baseline_rps = calculate_rps(baseline_df)
    solution_rps = calculate_rps(solution_df)
    ipvs_lc_rps = calculate_rps(ipvs_lc_df)

    ax.plot(
        baseline_rps["relative_time"],
        baseline_rps["metric_value"],
        label="BASELINE",
        linewidth=1.5,
        alpha=0.8,
    )
    ax.plot(
        solution_rps["relative_time"],
        solution_rps["metric_value"],
        label="EWMA",
        linewidth=1.5,
        alpha=0.8,
    )
    ax.plot(
        ipvs_lc_rps["relative_time"],
        ipvs_lc_rps["metric_value"],
        label="LEASTCONN",
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


def main():
    print("Loading k6 performance data...")
    baseline_df = load_and_clean_data(BASELINE_FILE, "BASELINE")
    solution_df = load_and_clean_data(SOLUTION_FILE, "EWMA")
    ipvs_lc_df = load_and_clean_data(IPVS_LC_FILE, "LEASTCONN")

    print(f"\nBASELINE: {len(baseline_df)} records after removing warmup")
    print(f"EWMA: {len(solution_df)} records after removing warmup")
    print(f"LEASTCONN: {len(ipvs_lc_df)} records after removing warmup")

    # Generate visualizations
    print("\nGenerating visualizations...")

    print("  1. Response time over time...")
    plot_response_time_overtime(baseline_df, solution_df, ipvs_lc_df)

    print("  2. Requests per second over time...")
    plot_rps_overtime(baseline_df, solution_df, ipvs_lc_df)


if __name__ == "__main__":
    main()
