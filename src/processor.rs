use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::models::SensorData;

const ANOMALY_THRESHOLD: f64 = 85.0;

/// Чистая функция, определяющая аномалию – легко тестируется.
fn is_anomaly(value: f64) -> bool {
    value > ANOMALY_THRESHOLD
}

pub async fn run(mut rx: mpsc::Receiver<SensorData>) -> anyhow::Result<()> {
    while let Some(data) = rx.recv().await {
        tracing::debug!(
            sensor_id = %data.sensor_id,
            value = data.value,
            "Received sensor data"
        );

        if is_anomaly(data.value) {
            warn!(
                sensor_id = %data.sensor_id,
                value = data.value,
                threshold = ANOMALY_THRESHOLD,
                timestamp = %data.timestamp,
                "ANOMALY DETECTED"
            );
        } else {
            info!(
                sensor_id = %data.sensor_id,
                value = data.value,
                "Normal reading"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // Модульные тесты чистой логики
    #[test]
    fn test_anomaly_detected() {
        assert!(is_anomaly(85.1));
    }

    #[test]
    fn test_normal_value_not_anomaly() {
        assert!(!is_anomaly(85.0));
    }

    #[test]
    fn test_boundary_exactly_threshold() {
        assert!(!is_anomaly(85.0));
    }

    // Интеграционный асинхронный тест: processor получает данные и завершается
    #[tokio::test]
    async fn test_processor_receives_and_completes() {
        let (tx, rx) = mpsc::channel(16);
        let test_data = SensorData {
            sensor_id: "test-001".to_string(),
            value: 90.0,
            timestamp: Utc::now(),
        };
        tx.send(test_data).await.expect("send");
        drop(tx); // закрываем канал, processor завершит работу
        let result = run(rx).await;
        assert!(result.is_ok());
    }
}
