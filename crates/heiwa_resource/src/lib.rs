//! Local machine resource snapshots and admission policy for Heiwa.
//!
//! This crate is intentionally pure policy. Runtime-specific telemetry
//! collection belongs in `heiwa-shell` or another host adapter; this crate only
//! decides whether a class of work should run, throttle, or stop.

use serde::{Deserialize, Serialize};

const GIB: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ResourceSnapshot {
    pub cpu_count: u32,
    pub load_1m: f32,
    pub free_memory_bytes: u64,
    pub battery_percent: Option<u8>,
    pub on_battery: bool,
    pub thermal_pressure: ThermalPressure,
}

impl ResourceSnapshot {
    pub fn load_ratio(&self) -> f32 {
        self.load_1m / self.cpu_count.max(1) as f32
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThermalPressure {
    Nominal,
    Fair,
    Serious,
    Critical,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkClass {
    ForegroundInteractive,
    BackgroundWatch,
    LocalSummary,
    LocalModelSmall,
    LocalModelLarge,
    ProviderEscalation,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Admission {
    Allow,
    Throttle { reason: String, retry_after_ms: u64 },
    Deny { reason: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ResourcePolicy {
    pub soft_load_ratio: f32,
    pub hard_load_ratio: f32,
    pub min_free_memory_bytes: u64,
    pub min_battery_percent: u8,
    pub load_retry_after_ms: u64,
    pub battery_retry_after_ms: u64,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            soft_load_ratio: 0.70,
            hard_load_ratio: 0.90,
            min_free_memory_bytes: 6 * GIB,
            min_battery_percent: 25,
            load_retry_after_ms: 30_000,
            battery_retry_after_ms: 60_000,
        }
    }
}

impl ResourcePolicy {
    pub fn admit(&self, snapshot: &ResourceSnapshot, class: WorkClass) -> Admission {
        if matches!(snapshot.thermal_pressure, ThermalPressure::Critical) {
            return self.deny_or_throttle_for_local(class, "thermal_pressure_critical");
        }

        if matches!(snapshot.thermal_pressure, ThermalPressure::Serious)
            && matches!(class, WorkClass::LocalModelLarge)
        {
            return Admission::Deny {
                reason: "thermal_pressure_serious".to_string(),
            };
        }

        if snapshot.load_ratio() >= self.hard_load_ratio {
            return self.deny_or_throttle_for_local(class, "load_above_hard_limit");
        }

        if snapshot.free_memory_bytes < self.min_free_memory_bytes
            && matches!(class, WorkClass::LocalModelLarge)
        {
            return Admission::Deny {
                reason: "free_memory_below_minimum".to_string(),
            };
        }

        if snapshot.on_battery
            && snapshot
                .battery_percent
                .is_some_and(|level| level < self.min_battery_percent)
            && Self::is_nonurgent_local_work(class)
        {
            return Admission::Throttle {
                reason: "battery_below_minimum".to_string(),
                retry_after_ms: self.battery_retry_after_ms,
            };
        }

        if snapshot.load_ratio() >= self.soft_load_ratio
            && matches!(class, WorkClass::BackgroundWatch)
        {
            return Admission::Throttle {
                reason: "load_above_soft_limit".to_string(),
                retry_after_ms: self.load_retry_after_ms,
            };
        }

        Admission::Allow
    }

    fn deny_or_throttle_for_local(&self, class: WorkClass, reason: &str) -> Admission {
        match class {
            WorkClass::LocalModelLarge => Admission::Deny {
                reason: reason.to_string(),
            },
            WorkClass::BackgroundWatch | WorkClass::LocalSummary | WorkClass::LocalModelSmall => {
                Admission::Throttle {
                    reason: reason.to_string(),
                    retry_after_ms: self.load_retry_after_ms,
                }
            }
            WorkClass::ForegroundInteractive | WorkClass::ProviderEscalation => Admission::Allow,
        }
    }

    fn is_nonurgent_local_work(class: WorkClass) -> bool {
        matches!(
            class,
            WorkClass::BackgroundWatch
                | WorkClass::LocalSummary
                | WorkClass::LocalModelSmall
                | WorkClass::LocalModelLarge
        )
    }
}
