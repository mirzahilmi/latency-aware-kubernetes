use anyhow::{Result, anyhow};
use regex::Regex;
use std::collections::HashMap;
use tokio::{process::Command, task::JoinSet};

pub async fn ping(targets: Vec<String>, n: u32) -> Result<HashMap<String, f64>> {
    let mut tasks = JoinSet::new();
    let regex = Regex::new(r"(?m)(?:time=){1}(.+)(?:\sms){1}$")?;

    for target in targets {
        let regex = regex.clone();
        tasks.spawn(async move {
            let output = Command::new("ping")
                .arg("-c")
                .arg(format!("{}", &n))
                .arg(&target)
                .output()
                .await?;

            if !output.status.success() {
                return Err(anyhow!(
                    "ping failed for {}: {}",
                    target,
                    String::from_utf8_lossy(&output.stderr)
                ));
            }

            let out = String::from_utf8(output.stdout)?;

            let mut metrics = vec![];
            for (_, [metric_str]) in regex.captures_iter(&out).map(|c| c.extract()) {
                if let Ok(metric) = metric_str.parse::<f32>() {
                    metrics.push(metric);
                }
            }

            if metrics.is_empty() {
                return Err(anyhow!("no ping metrics found for {}", target));
            }

            let avg = metrics.iter().sum::<f32>() as f64 / metrics.len() as f64;

            Ok((target, avg))
        });
    }

    let mut results = HashMap::new();
    while let Some(join_result) = tasks.join_next().await {
        match join_result {
            Ok(Ok((target, avg))) => {
                results.insert(target, avg);
            }
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(e.into()),
        }
    }

    Ok(results)
}
