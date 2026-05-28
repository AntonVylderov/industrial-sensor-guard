use anyhow::{Context, anyhow};
use chrono::Utc;
use std::net::SocketAddr;
use tokio::sync::broadcast;
use tokio_modbus::prelude::*;
use tokio_modbus::client::tcp;
use crate::models::{SensorReading, ReadingQuality};

pub struct ModbusConnector {
    pub addr: String,
    pub registers: Vec<(u16, String, String, String)>,
    pub interval_secs: u64,
}

impl ModbusConnector {
    pub async fn run(self, tx: broadcast::Sender<SensorReading>) -> anyhow::Result<()> {
        let socket_addr: SocketAddr = self.addr.parse()
            .context("Invalid Modbus server address")?;

        let mut retry_delay = tokio::time::Duration::from_secs(1);
        const MAX_RETRY_DELAY: tokio::time::Duration = tokio::time::Duration::from_secs(30);

        loop {
            let mut client = match tcp::connect(socket_addr).await {
                Ok(c) => {
                    tracing::info!("Modbus connected to {}", self.addr);
                    retry_delay = tokio::time::Duration::from_secs(1); // сбрасываем задержку при успехе
                    c
                }
                Err(e) => {
                    tracing::error!("Modbus connection failed: {}, retrying in {:?}", e, retry_delay);
                    tokio::time::sleep(retry_delay).await;
                    retry_delay = std::cmp::min(retry_delay * 2, MAX_RETRY_DELAY);
                    continue;
                }
            };

            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(self.interval_secs));
            let mut connection_error = false;

            loop {
                interval.tick().await;

                for (reg_addr, parameter, unit, reg_type) in &self.registers {
                    // tokio-modbus возвращает Result<Result<Vec<_>, ExceptionCode>, io::Error>
                    // Нужно раскрыть оба слоя Result через двойной ? или and_then
                    let result: anyhow::Result<Vec<u16>> = match reg_type.as_str() {
                        "input" => {
                            client.read_input_registers(*reg_addr, 1)
                                .await
                                .map_err(|e| anyhow!("Input register IO error: {}", e))
                                .and_then(|inner| {
                                    inner.map_err(|e| anyhow!("Input register exception: {:?}", e))
                                })
                        }

                        "coil" => {
                            client.read_coils(*reg_addr, 1)
                                .await
                                .map_err(|e| anyhow!("Coil IO error: {}", e))
                                .and_then(|inner| {
                                    inner.map_err(|e| anyhow!("Coil exception: {:?}", e))
                                })
                                .map(|coils: Vec<bool>| {
                                    coils.into_iter()
                                        .map(|b| if b { 1u16 } else { 0u16 })
                                        .collect()
                                })
                        }

                        _ => {
                            client.read_holding_registers(*reg_addr, 1)
                                .await
                                .map_err(|e| anyhow!("Holding register IO error: {}", e))
                                .and_then(|inner| {
                                    inner.map_err(|e| anyhow!("Holding register exception: {:?}", e))
                                })
                        }
                    };

                    match result {
                        Ok(data) => {
                            // Аннотация типа снимает E0282: компилятор знает что data: Vec<u16>
                            if let Some(&value) = data.first() {
                                let reading = SensorReading {
                                    source: format!(
                                        "modbus:{}/register/{}/{}",
                                        self.addr, reg_addr, reg_type
                                    ),
                                    parameter: parameter.clone(),
                                    value: value as f64,
                                    unit: unit.clone(),
                                    quality: ReadingQuality::Good,
                                    timestamp: Utc::now(),
                                };

                                if tx.send(reading).is_err() {
                                    tracing::error!("Modbus connector: broadcast channel closed, terminating");
                                    return Ok(());
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Read error at register {} ({}): {:?}",
                                reg_addr, reg_type, e
                            );
                            // Обрываем внутренний цикл → переподключение
                            connection_error = true;
                            break;
                        }
                    }
                }

                if connection_error {
                    break;
                }
            }

            tracing::warn!("Modbus connection lost, reconnecting in {:?}", retry_delay);
            tokio::time::sleep(retry_delay).await;
            retry_delay = std::cmp::min(retry_delay * 2, MAX_RETRY_DELAY);
        }
    }
}