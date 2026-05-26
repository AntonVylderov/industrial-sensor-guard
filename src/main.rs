mod error;
mod models;
mod processor;
mod producer;

use crate::models::SensorData;
use anyhow::Context;
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use tokio::signal;
use tokio::sync::mpsc;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ---------- Observability: логирование ----------
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(false)
                .with_span_list(false),
        )
        .init();

    tracing::info!("Starting Industrial Sensor Guard");

    // ---------- Observability: метрики Prometheus ----------
    let metrics_addr: SocketAddr = "0.0.0.0:9001".parse()?;
    tracing::info!(%metrics_addr, "Starting Prometheus metrics endpoint");

    PrometheusBuilder::new()
        .with_http_listener(metrics_addr)
        .install()
        .context("Failed to install Prometheus recorder")?;

    // ---------- Создание канала и запуск задач ----------
    let (tx, rx) = mpsc::channel::<SensorData>(32);

    let producer_handle = tokio::spawn(producer::run(tx));
    let processor_handle = tokio::spawn(processor::run(rx));

    // ---------- Graceful shutdown ----------
    signal::ctrl_c()
        .await
        .context("Failed to listen for Ctrl+C")?;

    tracing::info!("Shutdown signal received, stopping tasks...");

    producer_handle.abort();
    let _ = tokio::join!(async { producer_handle.await.ok() }, processor_handle,);

    tracing::info!("Industrial Sensor Guard shut down gracefully");
    Ok(())
}
