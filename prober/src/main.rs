use prober::{Prober, new_prober};

fn main() {
    let prober = new_prober(String::from("google.com"));
    let metrics = prober.ping(3).unwrap();

    for (i, metric) in metrics.iter().enumerate() {
        println!("Metric {i} is {metric}");
    }
}
