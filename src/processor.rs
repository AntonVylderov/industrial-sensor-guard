use crate::models::{SensorReading, AggregatedReading};
use chrono::{DateTime, Duration, Utc};
use metrics::counter;
use std::collections::{HashMap, VecDeque};
use tokio::sync::broadcast;
use tracing::{info, warn};

#[derive(Clone)]
struct ParamRule {
    threshold_warning: f64,
    threshold_critical: f64,
    window_secs: Option<f64>,
    duration_secs: Option<f64>,
    delta_per_minute: Option<f64>,
    heartbeat_secs: Option<f64>,
}

struct ParamState {
    samples: VecDeque<(DateTime<Utc>, f64)>,
    warning_start: Option<DateTime<Utc>>,
    critical_start: Option<DateTime<Utc>>,
    last_seen: DateTime<Utc>,
    last_value: Option<(DateTime<Utc>, f64)>,
}

impl ParamState {
    fn new(now: DateTime<Utc>) -> Self {
        Self {
            samples: VecDeque::new(),
            warning_start: None,
            critical_start: None,
            last_seen: now,
            last_value: None,
        }
    }

    fn add_sample(&mut self, now: DateTime<Utc>, value: f64, window_secs: Option<f64>) {
        self.last_seen = now;
        self.last_value = Some((now, value));
        self.samples.push_back((now, value));
        if let Some(win) = window_secs {
            let cutoff = now - Duration::from_std(std::time::Duration::from_secs_f64(win)).unwrap();
            while let Some(&(t, _)) = self.samples.front() {
                if t < cutoff {
                    self.samples.pop_front();
                } else {
                    break;
                }
            }
        } else {
            while self.samples.len() > 1 {
                self.samples.pop_front();
            }
        }
    }

    fn aggregated_value(&self, rule: &ParamRule) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        if rule.window_secs.is_some() {
            let sum: f64 = self.samples.iter().map(|(_, v)| v).sum();
            Some(sum / self.samples.len() as f64)
        } else {
            Some(self.samples.back().unwrap().1)
        }
    }

    fn check_delta(&self, rule: &ParamRule, now: DateTime<Utc>, current: f64) -> Option<String> {
        if let Some(max_delta) = rule.delta_per_minute {
            if let Some((prev_time, prev_value)) = self.last_value {
                let delta_minutes = (now - prev_time).num_seconds() as f64 / 60.0;
                if delta_minutes > 0.0 {
                    let change = (current - prev_value).abs();
                    if change > max_delta {
                        return Some(format!("Delta {:.2} exceeds max {:.2}/min", change, max_delta));
                    }
                }
            }
        }
        None
    }

    fn update_duration_alert(
        &mut self,
        now: DateTime<Utc>,
        is_exceeding: bool,
        duration_secs: f64,
        severity: &str,
    ) -> bool {
        let start = match severity {
            "warning" => &mut self.warning_start,
            "critical" => &mut self.critical_start,
            _ => return false,
        };
        if is_exceeding {
            if start.is_none() {
                *start = Some(now);
            } else if let Some(start_time) = start {
                if (now - *start_time).num_seconds() as f64 >= duration_secs {
                    *start = None;
                    return true;
                }
            }
        } else {
            *start = None;
        }
        false
    }
}

pub struct Processor {
    rules: HashMap<String, ParamRule>,
    states: HashMap<String, ParamState>,
    tx_agg: broadcast::Sender<AggregatedReading>,
}

impl Processor {
    pub fn new(
        rules: Vec<(String, f64, f64, Option<f64>, Option<f64>, Option<f64>, Option<f64>)>,
        tx_agg: broadcast::Sender<AggregatedReading>,
    ) -> Self {
        let mut rule_map = HashMap::new();
        for (param, warn, crit, window, duration, delta, heartbeat) in rules {
            rule_map.insert(param, ParamRule {
                threshold_warning: warn,
                threshold_critical: crit,
                window_secs: window,
                duration_secs: duration,
                delta_per_minute: delta,
                heartbeat_secs: heartbeat,
            });
        }
        Self {
            rules: rule_map,
            states: HashMap::new(),
            tx_agg,
        }
    }

    pub async fn run(mut self, mut rx: broadcast::Receiver<SensorReading>) -> anyhow::Result<()> {
        let mut heartbeat_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Ok(reading) => self.process_reading(reading).await,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!("Processor lagged, skipped {} messages", skipped);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            info!("Channel closed, processor shutting down");
                            break;
                        }
                    }
                }
                _ = heartbeat_interval.tick() => {
                    self.check_heartbeats().await;
                }
            }
        }
        Ok(())
    }

    async fn process_reading(&mut self, reading: SensorReading) {
        let param = reading.parameter.clone();
        let now = reading.timestamp;
        let raw_value = reading.value;

        let rule = match self.rules.get(&param) {
            Some(r) => r,
            None => {
                counter!("sensor_readings_total", "parameter" => param).increment(1);
                return;
            }
        };

        let state = self.states.entry(param.clone()).or_insert_with(|| ParamState::new(now));
        state.add_sample(now, raw_value, rule.window_secs);

        let value = match state.aggregated_value(rule) {
            Some(v) => v,
            None => return,
        };

        // Отправляем агрегированное значение в UI
        let agg_reading = AggregatedReading {
            parameter: param.clone(),
            value,
            unit: reading.unit.clone(),
            timestamp: now,
        };
        if self.tx_agg.send(agg_reading).is_err() {
            warn!("No receivers for aggregated readings (parameter: {})", param);
        }

        counter!("sensor_readings_total", "parameter" => param.clone()).increment(1);

        if let Some(delta_msg) = state.check_delta(rule, now, value) {
            counter!("sensor_alerts_total", "parameter" => param.clone(), "severity" => "delta").increment(1);
            warn!(
                source = %reading.source,
                parameter = %param,
                value = value,
                delta_msg = %delta_msg,
                "Delta alert"
            );
        }

        let is_critical = value >= rule.threshold_critical;
        let is_warning = value >= rule.threshold_warning;

        if let Some(duration) = rule.duration_secs {
            if state.update_duration_alert(now, is_critical, duration, "critical") {
                counter!("sensor_alerts_total", "parameter" => param.clone(), "severity" => "critical_duration").increment(1);
                warn!(
                    source = %reading.source,
                    parameter = %param,
                    value = value,
                    duration_secs = duration,
                    threshold_critical = rule.threshold_critical,
                    "CRITICAL alert (duration exceeded)"
                );
            } else if state.update_duration_alert(now, is_warning && !is_critical, duration, "warning") {
                counter!("sensor_alerts_total", "parameter" => param.clone(), "severity" => "warning_duration").increment(1);
                warn!(
                    source = %reading.source,
                    parameter = %param,
                    value = value,
                    duration_secs = duration,
                    threshold_warning = rule.threshold_warning,
                    "WARNING alert (duration exceeded)"
                );
            }
        } else {
            if is_critical {
                counter!("sensor_alerts_total", "parameter" => param.clone(), "severity" => "critical").increment(1);
                warn!(
                    source = %reading.source,
                    parameter = %param,
                    value = value,
                    threshold_critical = rule.threshold_critical,
                    "CRITICAL alert"
                );
            } else if is_warning {
                counter!("sensor_alerts_total", "parameter" => param.clone(), "severity" => "warning").increment(1);
                warn!(
                    source = %reading.source,
                    parameter = %param,
                    value = value,
                    threshold_warning = rule.threshold_warning,
                    "Warning alert"
                );
            } else {
                info!(
                    source = %reading.source,
                    parameter = %param,
                    value = value,
                    "Normal reading"
                );
            }
        }
    }

    async fn check_heartbeats(&mut self) {
        let now = Utc::now();
        for (param, state) in self.states.iter() {
            if let Some(rule) = self.rules.get(param) {
                if let Some(heartbeat_secs) = rule.heartbeat_secs {
                    let elapsed = (now - state.last_seen).num_seconds() as f64;
                    if elapsed > heartbeat_secs {
                        counter!("sensor_heartbeat_failures", "parameter" => param.clone()).increment(1);
                        warn!(
                            parameter = %param,
                            last_seen_secs_ago = elapsed,
                            heartbeat_secs = heartbeat_secs,
                            "Heartbeat alert: no data received"
                        );
                    }
                }
            }
        }
    }
}