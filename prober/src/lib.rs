use regex::Regex;
use std::{error::Error, process::Command, sync::Arc, thread, vec};

use crate::prober::Latency;

pub mod prober {
    include!(concat!(env!("OUT_DIR"), "/prober.rs"));
}

pub trait Prober {
    fn ping(&self, n: usize) -> Result<Vec<Latency>, Box<dyn Error>>;
}

pub struct Ping {
    pub targets: Vec<String>,
}

impl Prober for Ping {
    fn ping(&self, n: usize) -> Result<Vec<Latency>, Box<dyn Error>> {
        let mut tasks = vec![];
        let pattern = Arc::new(Regex::new(r"(?m)(?:time=){1}(.+)(?:\sms){1}$")?);

        for target in self.targets.clone() {
            let pattern = Arc::clone(&pattern);
            tasks.push(thread::spawn(move || -> (String, Vec<f32>) {
                let out = Command::new("ping")
                    .arg("-c")
                    .arg(format!("{}", &n))
                    .arg(&target)
                    .output()
                    .unwrap();
                let out = String::from_utf8(out.stdout).unwrap();

                let entries: String = out
                    .split("\n")
                    .enumerate()
                    .filter(|(i, _)| (1..n + 1).contains(i))
                    .map(|(_, v)| -> String {
                        let mut v = String::from(v);
                        v.push('\n');
                        v
                    })
                    .collect::<Vec<String>>()
                    .concat();

                let mut metrics = vec![];
                for (_, [metric]) in pattern.captures_iter(&entries).map(|c| c.extract()) {
                    let metric: f32 = metric.parse().unwrap();
                    metrics.push(metric);
                }

                (target, metrics)
            }));
        }

        let mut latencies = vec![];
        for task in tasks {
            let (target, metrics) = match task.join() {
                Ok(it) => it,
                Err(err) => {
                    let msg = match err.downcast_ref::<&'static str>() {
                        Some(s) => *s,
                        None => match err.downcast_ref::<String>() {
                            Some(s) => &s[..],
                            None => "Sorry, unknown payload type",
                        },
                    };
                    println!("Thread panics: {msg}");
                    continue;
                }
            };
            latencies.push(Latency {
                ip_destination: target,
                metrics,
            });
        }

        Ok(latencies)
    }
}
