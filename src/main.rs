mod error;
mod models;
mod processor;
mod producer;

use crate::models::SensorData;
use anyhow::Context;
use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::mpsc;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ---------- Логирование ----------
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

    // ---------- Метрики Prometheus ----------
    let metrics_addr: SocketAddr = "0.0.0.0:9001".parse()?;
    tracing::info!(%metrics_addr, "Starting Prometheus metrics endpoint");

    let handle = PrometheusBuilder::new()
        .install_recorder()
        .context("Failed to install Prometheus recorder")?;

    let listener = TcpListener::bind(metrics_addr).await?;
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let handle = handle.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |_req| {
                    let metrics = handle.render();
                    let body = Full::new(Bytes::from(metrics));
                    async move { Ok::<_, hyper::Error>(hyper::Response::new(body)) }
                });
                if let Err(e) = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), svc)
                    .await
                {
                    tracing::error!("Metrics connection error: {}", e);
                }
            });
        }
    });

    // ---------- Канал и задачи ----------
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
