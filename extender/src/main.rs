mod handlers;
mod models;
mod state;
mod kube_client;

use axum::{routing::post, routing::get, Router};
use std::net::SocketAddr;
use tracing::{info};
use tracing_subscriber::FmtSubscriber;
use std::sync::Arc;
use crate::handlers::{filter_handler, prioritize_handler};
use crate::state::AppState;

async fn healthz() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!(">>> Extender starting...");

    // Initialize tracing with environment variable support
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("extender=debug".parse().expect("invalid log level"))
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");
    
    eprintln!(">>> Tracing init done");

    let state = Arc::new(AppState::new());

    // Periodic updater (refresh mapping + pull scores dari prober busiest node)
    let updater = state.clone();
    tokio::spawn(async move {
        if let Err(e) = updater.periodic_update_loop().await {
            tracing::error!("Periodic update loop stopped: {}", e);
        }
    });
    eprintln!(">>> State initialized, building router");

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/filter", post(filter_handler))
        .route("/prioritize", post(prioritize_handler))
        .with_state(state);   

    let addr: SocketAddr = "0.0.0.0:3001".parse().expect("Failed to parse socket addr");
    eprintln!(">>> Binding to {}", addr);
    info!("Extender HTTP server listening on {}", addr);
    info!("Endpoints: /healthz, /filter, /prioritize");

    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        app
    )
    .await?;

    eprintln!(">>> Server exited (should never reach here!)");
    Ok(())
}
