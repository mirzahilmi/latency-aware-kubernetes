use std::{collections::HashMap, net::Ipv4Addr};

use crate::config::Config;

use super::actor::{Event, EwmaDatapoint};
use tokio::task;
use tokio::{
    sync::broadcast,
    time::{Duration, Instant, interval},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub async fn probe_latency(
    config: Config,
    tx: broadcast::Sender<Event>,
    token: CancellationToken,
) -> anyhow::Result<()> {
    let mut ticker = interval(Duration::from_secs(config.probe.latency_interval));
    let mut endpoints_by_nodename = HashMap::<String, Vec<Ipv4Addr>>::new();
    let mut datapoint_by_nodename = HashMap::<String, f64>::new();

    let mut rx = tx.subscribe();
    'main: loop {
        // hentikan main loop ketika program shutdown
        if token.is_cancelled() {
            info!("actor: exiting latency_probe task");
            return Ok(());
        }

        // mencoba membaca event perubahan Service pada channel
        while let Ok(event) = rx.try_recv() {
            if let Event::ServiceChanged(service) = event {
                endpoints_by_nodename = service.endpoints_by_nodename;
            };
        }

        let mut handles = Vec::new();
        endpoints_by_nodename
            .iter()
            // melakukan request laman / pada salah satu pod aplikasi yang berjalan pada setiap node
            .for_each(|(nodename, endpoints)| {
                let nodename = nodename.clone();
                let endpoints = endpoints.clone();

                handles.push(task::spawn(async move {
                    let mut response_time_ms: Option<u128> = None;
                    for endpoint in endpoints {
                        // inisialisasi waktu sebelum request laman dimulai
                        let now = Instant::now();
                        // melakukan request laman /
                        if (reqwest::get(format!(
                            "http://{}:{}",
                            endpoint, config.kubernetes.target_port
                        ))
                        .await)
                            .is_ok()
                        {
                            // menghitung waktu respon semenjak waktu inisialisasi
                            response_time_ms = Some(now.elapsed().as_millis());
                            break;
                        };
                    }
                    (nodename, response_time_ms)
                }));
            });

        // membaca semua hasil pengukuran waktu respon pada proses sebelumnya
        let mut response_times = Vec::new();
        for handle in handles {
            let result = match handle.await {
                Ok(result) => result,
                Err(e) => {
                    error!("actor: failed to execute task: {e}");
                    continue;
                }
            };
            response_times.push(result);
        }

        // menghitung skor EWMA untuk setiap hasil waktu respon dan mengirim kumpulan skor tersebut
        // melalui channel sebagai event EwmaCalculated
        for (nodename, response_time) in response_times {
            let Some(elapsed_ms) = response_time else {
                warn!("actor: failed to probe latency for any endpoints available @ {nodename}");
                continue;
            };

            debug!(
                "actor: latency probe of {} takes {} ms",
                nodename, elapsed_ms
            );

            let elapsed_ms = elapsed_ms as f64;
            let datapoint = match datapoint_by_nodename.get(&nodename) {
                // kalkulasi skor EWMA ketika terdapat skor pada titik sebelumnya
                Some(datapoint) => {
                    config.alpha.ewma_latency * elapsed_ms
                        + (1.0 - config.alpha.ewma_latency) * *datapoint
                }
                // gunakan waktu respon mentah sebagai skor EWMA
                // ketika tidak ada skor pada titik penghitungan sebelumnya
                None => elapsed_ms,
            };

            // menyimpan nilai skor EWMA per node untuk digunakan pada perhitungan skor selanjutnya
            datapoint_by_nodename.insert(nodename.clone(), datapoint);

            // mengirim hasil skor EWMA untuk metrik waktu respon untuk
            // setiap node sebagai event EwmaCalculated melalui channel
            if let Err(e) = tx.send(Event::EwmaCalculated(
                nodename.clone(),
                EwmaDatapoint::Latency(datapoint),
            )) {
                info!("actor: latency probe exiting: {e}");
                break 'main;
            };
        }

        // memberhentikan sementara eksekusi loop selanjutnya
        // dalam kurun waktu yang ditentukan dari konfigurasi
        ticker.tick().await;
    }

    Ok(())
}
