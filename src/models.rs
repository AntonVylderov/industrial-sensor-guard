use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Структура, описывающая одно измерение промышленного датчика.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    pub sensor_id: String,
    pub value: f64,
    pub timestamp: DateTime<Utc>,
}
