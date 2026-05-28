use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReadingQuality {
    Good,
    Uncertain,
    Bad,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReading {
    pub source: String,
    pub parameter: String,
    pub value: f64,
    pub unit: String,
    pub quality: ReadingQuality,
    pub timestamp: DateTime<Utc>,
}

/// Агрегированное (сглаженное) значение для отображения в UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedReading {
    pub parameter: String,
    pub value: f64,
    pub unit: String,
    pub timestamp: DateTime<Utc>,
}

// Оставлен для обратной совместимости (если используется в тестах)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SensorData {
    pub sensor_id: String,
    pub value: f64,
    pub timestamp: DateTime<Utc>,
}