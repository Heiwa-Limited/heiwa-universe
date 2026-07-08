use heiwa_resource::{Admission, ResourcePolicy, ResourceSnapshot, ThermalPressure, WorkClass};

fn base() -> ResourceSnapshot {
    ResourceSnapshot {
        cpu_count: 12,
        load_1m: 3.0,
        free_memory_bytes: 10 * 1024 * 1024 * 1024,
        battery_percent: Some(80),
        on_battery: false,
        thermal_pressure: ThermalPressure::Nominal,
    }
}

#[test]
fn allows_local_summary_when_machine_is_healthy() {
    let admission = ResourcePolicy::default().admit(&base(), WorkClass::LocalSummary);

    assert_eq!(admission, Admission::Allow);
}

#[test]
fn throttles_background_work_under_soft_load_pressure() {
    let snapshot = ResourceSnapshot {
        load_1m: 9.0,
        ..base()
    };

    let admission = ResourcePolicy::default().admit(&snapshot, WorkClass::BackgroundWatch);

    assert_eq!(
        admission,
        Admission::Throttle {
            reason: "load_above_soft_limit".to_string(),
            retry_after_ms: 30_000,
        }
    );
}

#[test]
fn denies_large_local_model_when_load_exceeds_hard_limit() {
    let snapshot = ResourceSnapshot {
        load_1m: 11.0,
        ..base()
    };

    let admission = ResourcePolicy::default().admit(&snapshot, WorkClass::LocalModelLarge);

    assert_eq!(
        admission,
        Admission::Deny {
            reason: "load_above_hard_limit".to_string(),
        }
    );
}

#[test]
fn denies_large_local_model_when_free_memory_is_low() {
    let snapshot = ResourceSnapshot {
        free_memory_bytes: 2 * 1024 * 1024 * 1024,
        ..base()
    };

    let admission = ResourcePolicy::default().admit(&snapshot, WorkClass::LocalModelLarge);

    assert_eq!(
        admission,
        Admission::Deny {
            reason: "free_memory_below_minimum".to_string(),
        }
    );
}

#[test]
fn throttles_nonurgent_local_work_on_low_battery() {
    let snapshot = ResourceSnapshot {
        battery_percent: Some(15),
        on_battery: true,
        ..base()
    };

    let admission = ResourcePolicy::default().admit(&snapshot, WorkClass::LocalSummary);

    assert_eq!(
        admission,
        Admission::Throttle {
            reason: "battery_below_minimum".to_string(),
            retry_after_ms: 60_000,
        }
    );
}

#[test]
fn denies_large_local_model_under_serious_thermal_pressure() {
    let snapshot = ResourceSnapshot {
        thermal_pressure: ThermalPressure::Serious,
        ..base()
    };

    let admission = ResourcePolicy::default().admit(&snapshot, WorkClass::LocalModelLarge);

    assert_eq!(
        admission,
        Admission::Deny {
            reason: "thermal_pressure_serious".to_string(),
        }
    );
}
