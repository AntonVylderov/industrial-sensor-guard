use anyhow::Context;
use chrono::Utc;
use rand::prelude::*;
use rand::rngs::StdRng;
use tokio::sync::mpsc;

use crate::error::ServiceError;
use crate::models::SensorData;

pub async fn run(tx: mpsc::Sender<SensorData>) -> anyhow::Result<()> {
    // StdRng: Send + Sync, безопасен для tokio::spawn.
    // Инициализация через rand::rng() (потокобезопасный источник энтропии).
    let mut rng = StdRng::from_rng(&mut rand::rng());
    let sensor_ids = ["temp-001", "press-002", "vibro-003"];

    loop {
        let sensor_id = sensor_ids
            .choose(&mut rng)
            .context("Empty sensor list")?
            .to_string();
        let value = rng.random_range(70.0..100.0);
        let data = SensorData {
            sensor_id,
            value,
            timestamp: Utc::now(),
        };

        if let Err(e) = tx.send(data).await {
            return Err(ServiceError::ChannelSend(e.to_string()).into());
        }

        let delay_ms = rng.random_range(300..800);
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
    }
}
