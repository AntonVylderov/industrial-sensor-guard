mod modbus_connector;
mod models;
mod processor;
mod simulator;
mod web_dashboard;

use crate::models::{SensorReading, AggregatedReading};
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
use tokio::sync::broadcast;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use tracing_appender::non_blocking::WorkerGuard;

const BROADCAST_CAPACITY: usize = 1024;
const METRICS_ADDR: &str = "0.0.0.0:9001";

#[derive(serde::Deserialize)]
struct Config {
    mode: String,
    rules: Vec<RuleConfig>,
    modbus: Option<ModbusConfig>,
}

#[derive(serde::Deserialize)]
struct RuleConfig {
    parameter: String,
    threshold_warning: f64,
    threshold_critical: f64,
    #[serde(rename = "unit")]
    _unit: String,
    #[serde(default)]
    window_secs: Option<f64>,
    #[serde(default)]
    duration_secs: Option<f64>,
    #[serde(default)]
    delta_per_minute: Option<f64>,
    #[serde(default)]
    heartbeat_secs: Option<f64>,
}

#[derive(serde::Deserialize)]
struct ModbusConfig {
    addr: String,
    interval_secs: u64,
    registers: Vec<RegisterConfig>,
}

#[derive(serde::Deserialize)]
struct RegisterConfig {
    address: u16,
    parameter: String,
    unit: String,
    #[serde(default = "default_register_type")]
    register_type: String,
}

fn default_register_type() -> String {
    "holding".to_string()
}

/// Инициализация логирования: одновременно в файл и в терминал (stdout).
/// Возвращает WorkerGuard — нужно держать до конца main, иначе файловый аппендер
/// не успеет сбросить буфер при выходе.
fn init_logging() -> WorkerGuard {
    let file_appender = tracing_appender::rolling::daily("logs", "app.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    // Слой → stdout (с цветами ANSI)
    let stdout_layer = fmt::layer()
        .with_target(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_ansi(true)
        .with_writer(std::io::stdout);

    // Слой → файл (без ANSI)
    let file_layer = fmt::layer()
        .with_target(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_ansi(false)
        .with_writer(file_writer);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();

    guard
}

async fn run_metrics_server() -> anyhow::Result<()> {
    let addr: SocketAddr = METRICS_ADDR.parse()?;
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .context("Failed to install Prometheus recorder")?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Prometheus metrics server listening on http://{}", addr);
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let handle = handle.clone();
                    tokio::spawn(async move {
                        let svc = service_fn(move |_| {
                            let body = Full::new(Bytes::from(handle.render()));
                            async { Ok::<_, hyper::Error>(hyper::Response::new(body)) }
                        });
                        let _ = http1::Builder::new()
                            .serve_connection(TokioIo::new(stream), svc)
                            .await;
                    });
                }
                Err(e) => tracing::error!("Metrics server accept error: {}", e),
            }
        }
    });
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _log_guard = init_logging();
    tracing::info!("Starting Industrial Sensor Monitor");

    let config: Config = {
        let content = std::fs::read_to_string("config.yaml")
            .context("Failed to read config.yaml")?;
        serde_yaml::from_str(&content).context("Failed to parse config.yaml")?
    };
    tracing::info!("Configuration loaded (mode: {})", config.mode);

    if (config.mode == "modbus" || config.mode == "both") && config.modbus.is_none() {
        anyhow::bail!("Mode '{}' requires a 'modbus' section in config.yaml", config.mode);
    }

    run_metrics_server().await?;

    // Канал для сырых данных
    let (tx, _) = broadcast::channel::<SensorReading>(BROADCAST_CAPACITY);
    // Канал для агрегированных данных
    let (tx_agg, _) = broadcast::channel::<AggregatedReading>(BROADCAST_CAPACITY);

    let rules_for_processor: Vec<(String, f64, f64, Option<f64>, Option<f64>, Option<f64>, Option<f64>)> =
        config.rules
            .iter()
            .map(|r| (
                r.parameter.clone(),
                r.threshold_warning,
                r.threshold_critical,
                r.window_secs,
                r.duration_secs,
                r.delta_per_minute,
                r.heartbeat_secs,
            ))
            .collect();

    let tx_processor = tx.clone();
    let tx_simulator = tx.clone();
    let tx_modbus    = tx.clone();
    let tx_agg_processor = tx_agg.clone();

    // Веб-дашборд
    let tx_ui_raw = tx.subscribe();
    let tx_ui_agg = tx_agg.subscribe();
    tokio::spawn(async move {
        web_dashboard::run_web_dashboard(tx_ui_raw, tx_ui_agg, "0.0.0.0:3030").await;
    });

    // Processor
    let processor_instance = processor::Processor::new(rules_for_processor, tx_agg_processor);
    tokio::spawn(async move {
        if let Err(e) = processor_instance.run(tx_processor.subscribe()).await {
            tracing::error!("Processor terminated with error: {:?}", e);
        } else {
            tracing::info!("Processor shut down");
        }
    });

    // Simulator
    if config.mode == "simulator" || config.mode == "both" {
        tokio::spawn(async move {
            if let Err(e) = simulator::run(tx_simulator).await {
                tracing::error!("Simulator terminated with error: {:?}", e);
            } else {
                tracing::info!("Simulator shut down");
            }
        });
        tracing::info!("Simulator started");
    }

    // Modbus connector
    if config.mode == "modbus" || config.mode == "both" {
        let mb_config = config.modbus.unwrap();
        let connector = modbus_connector::ModbusConnector {
            addr: mb_config.addr.clone(),
            registers: mb_config.registers
                .iter()
                .map(|r| (r.address, r.parameter.clone(), r.unit.clone(), r.register_type.clone()))
                .collect(),
            interval_secs: mb_config.interval_secs,
        };
        tokio::spawn(async move {
            if let Err(e) = connector.run(tx_modbus).await {
                tracing::error!("Modbus connector terminated with error: {:?}", e);
            } else {
                tracing::info!("Modbus connector shut down");
            }
        });
        tracing::info!("Modbus connector started (server: {})", mb_config.addr);
    }

    tracing::info!("Application running. Web dashboard: http://localhost:3030 | Metrics: http://localhost:9001");
    tracing::info!("Press Ctrl+C to stop.");
    signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received.");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    tracing::info!("Goodbye");
    Ok(())
}