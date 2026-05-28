use crate::models::{ReadingQuality, SensorReading};
use chrono::Utc;
use rand::prelude::*;
use tokio::sync::broadcast;
use tracing::debug;

pub async fn run(tx: broadcast::Sender<SensorReading>) -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(42);
    let parameters = [
        ("bearing_vibration_x", "mm/s"),
        ("bearing_temperature", "°C"),
        ("motor_current", "A"),
    ];

    debug!("Simulator started");
    loop {
        let (param, unit) = parameters.choose(&mut rng).unwrap();
        let value = match *param {
            "bearing_vibration_x" => rng.random_range(1.0..8.0),
            "bearing_temperature" => rng.random_range(60.0..90.0),
            "motor_current" => rng.random_range(8.0..16.0),
            _ => 0.0,
        };

        let reading = SensorReading {
            source: "simulator".to_string(),
            parameter: param.to_string(),
            value,
            unit: unit.to_string(),
            quality: ReadingQuality::Good,
            timestamp: Utc::now(),
        };

        debug!("Sending reading: {:?}", reading);
        if tx.send(reading).is_err() {
            break;
        }

        let delay_ms = rng.random_range(300..800);
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
    }
    Ok(())
}