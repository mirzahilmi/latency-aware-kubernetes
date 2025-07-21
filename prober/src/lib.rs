use std::process::Command;

use regex::Regex;

pub type Error = Box<dyn std::error::Error>;

pub trait Prober {
    fn ping(&self, n: usize) -> Result<Vec<f64>, Error>;
}

// this one actually the usual pattern i do when writing golang, im not sure if this also
// appropriate when writing rust, but at the time i write this, it is what it is
pub fn new_prober(target: String) -> impl Prober {
    Ping { target }
}

struct Ping {
    target: String,
}

impl Prober for Ping {
    fn ping(&self, n: usize) -> Result<Vec<f64>, Error> {
        let out = Command::new("ping")
            .arg("-c")
            .arg(format!("{n}"))
            .arg(&self.target)
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

        let pattern = Regex::new(r"(?m)(?:time=){1}(.+)(?:\sms){1}$").unwrap();

        let mut metrics = vec![];
        for (_, [metric]) in pattern.captures_iter(&entries).map(|c| c.extract()) {
            let metric: f64 = metric.parse()?;
            metrics.push(metric);
        }

        Ok(metrics)
    }
}
