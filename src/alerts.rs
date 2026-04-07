use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ThresholdConfig {
    pub interface: String,
    pub inbound_kb: Option<f64>,
    pub outbound_kb: Option<f64>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct AlertEvent {
    pub interface: String,
    pub direction: String, // "inbound" or "outbound"
    pub value_kb: f64,
    pub threshold_kb: f64,
    pub timestamp: u64, // unix seconds
}

impl AlertEvent {
    pub fn new(interface: &str, direction: &str, value_kb: f64, threshold_kb: f64) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            interface: interface.to_string(),
            direction: direction.to_string(),
            value_kb,
            threshold_kb,
            timestamp,
        }
    }
}

/// Check all interfaces against configured thresholds and return any new breaches.
pub fn check_thresholds(
    interfaces: &[crate::app::InterfaceStats],
    thresholds: &[ThresholdConfig],
) -> Vec<AlertEvent> {
    let mut events = Vec::new();
    for iface in interfaces {
        for t in thresholds {
            if t.interface != iface.name && t.interface != "*" {
                continue;
            }
            if let Some(limit) = t.inbound_kb {
                if iface.speed_in > limit {
                    events.push(AlertEvent::new(
                        &iface.name,
                        "inbound",
                        iface.speed_in,
                        limit,
                    ));
                }
            }
            if let Some(limit) = t.outbound_kb {
                if iface.speed_out > limit {
                    events.push(AlertEvent::new(
                        &iface.name,
                        "outbound",
                        iface.speed_out,
                        limit,
                    ));
                }
            }
        }
    }
    events
}
