use serde_json::Value;

use crate::types::{
    AlertLevel, AmsHumidity, AmsSlot, PrinterAlert, PrinterProfile, PrinterSnapshot,
    PrinterStatus,
};

pub fn merge_report(
    profile: &PrinterProfile,
    current: Option<PrinterSnapshot>,
    payload: &Value,
) -> PrinterSnapshot {
    let mut snapshot = current.unwrap_or_else(|| PrinterSnapshot::offline(profile, "未取得"));
    snapshot.profile_id = Some(profile.id.clone());
    snapshot.printer_name = profile.name.clone();
    snapshot.mode = profile.mode.clone();
    snapshot.connected = true;
    snapshot.last_updated = Some(chrono::Utc::now().to_rfc3339());

    let Some(print) = payload.get("print") else {
        return snapshot;
    };

    if let Some(state) = str_field(print, "gcode_state") {
        snapshot.raw_state = Some(state.clone());
        snapshot.status = map_status(&state);
        snapshot.status_label = status_label(&snapshot.status, &state).to_string();
    }

    if let Some(value) = u64_field(print, "mc_percent") {
        snapshot.progress = value.min(100) as u8;
    }
    if let Some(value) = u64_field(print, "mc_remaining_time") {
        snapshot.remaining_minutes = Some(value);
    }
    if let Some(value) = u64_field(print, "layer_num") {
        snapshot.current_layer = Some(value);
    }
    if let Some(value) = u64_field(print, "total_layer_num") {
        snapshot.total_layers = Some(value);
    }
    if let Some(value) = f64_field(print, "nozzle_temper") {
        snapshot.nozzle_temp = Some(value);
    }
    if let Some(value) = f64_field(print, "nozzle_target_temper") {
        snapshot.nozzle_target = Some(value);
    }
    if let Some(value) = f64_field(print, "bed_temper") {
        snapshot.bed_temp = Some(value);
    }
    if let Some(value) = f64_field(print, "bed_target_temper") {
        snapshot.bed_target = Some(value);
    }
    if let Some(value) = f64_field_deep_any(
        print,
        &[
            "chamber_temper",
            "chamber_temp",
            "chamber_temperature",
            "chamber_temper_curr",
            "chamber_temperature_curr",
            "mc_chamber_temper",
            "mc_chamber_temp",
            "ctt",
        ],
    ) {
        snapshot.chamber_temp = Some(value);
    }
    // Newer Bambu firmware: device.ctc.info.temp
    if snapshot.chamber_temp.is_none() {
        if let Some(t) = print
            .get("device")
            .and_then(|d| d.get("ctc"))
            .and_then(|ctc| ctc.get("info"))
            .and_then(|info| info.get("temp"))
            .and_then(|v| v.as_f64())
            .filter(|&t| t > 0.0)
        {
            snapshot.chamber_temp = Some(t);
        }
    }
    // Fallback: print.info.temp
    if snapshot.chamber_temp.is_none() {
        if let Some(t) = print
            .get("info")
            .and_then(|info| info.get("temp"))
            .and_then(|v| v.as_f64())
            .filter(|&t| t > 0.0)
        {
            snapshot.chamber_temp = Some(t);
        }
    }
    if let Some(value) = f64_field_deep_any(
        print,
        &[
            "chamber_target_temper",
            "chamber_target_temp",
            "chamber_temperature_tgt",
            "chamber_temper_tgt",
            "ctt_tgt",
        ],
    ) {
        snapshot.chamber_target = Some(value);
    }
    if snapshot.chamber_target.is_none() {
        if let Some(t) = print
            .get("device")
            .and_then(|d| d.get("ctc"))
            .and_then(|ctc| ctc.get("info"))
            .and_then(|info| info.get("target"))
            .and_then(|v| v.as_f64())
        {
            snapshot.chamber_target = Some(t);
        }
    }

    if let Some(value) = u64_field(print, "spd_mag") {
        snapshot.speed_percent = Some(value);
    }
    if let Some(value) = u64_field(print, "spd_lvl") {
        snapshot.speed_level = Some(value);
    }
    if let Some(value) = str_field(print, "wifi_signal") {
        snapshot.wifi_signal = Some(value);
    }

    let job_name = str_field(print, "subtask_name")
        .filter(|value| !value.is_empty())
        .or_else(|| str_field(print, "gcode_file").filter(|value| !value.is_empty()));
    if job_name.is_some() {
        snapshot.job_name = job_name;
    }

    if let Some(slots) = parse_ams_slots(print) {
        snapshot.ams_slots = slots;
    }
    if let Some(humidity) = parse_ams_humidity(print) {
        snapshot.ams_humidity = humidity;
    }

    snapshot.alerts = parse_alerts(print);
    if snapshot
        .alerts
        .iter()
        .any(|alert| matches!(alert.level, AlertLevel::Error))
    {
        snapshot.status = PrinterStatus::Failed;
        snapshot.status_label = "エラー".to_string();
    }
    snapshot
}

fn parse_ams_slots(print: &Value) -> Option<Vec<AmsSlot>> {
    let ams = print.get("ams")?;
    let active = u64_field(ams, "tray_now");
    let units = ams.get("ams")?.as_array()?;
    let mut slots = Vec::new();
    for unit in units {
        let ams_id = u64_field(unit, "id").unwrap_or(slots.len() as u64);
        let Some(trays) = unit.get("tray").and_then(Value::as_array) else {
            continue;
        };
        for (tray_index, tray) in trays.iter().enumerate() {
            let tray_id = u64_field(tray, "id").unwrap_or(0);
            let color = normalize_color(
                str_field(tray, "tray_color")
                    .as_deref()
                    .unwrap_or("00000000"),
            );
            let filament_type = str_field(tray, "tray_type").filter(|value| !value.is_empty());
            let sub_brand = str_field(tray, "tray_sub_brands").filter(|value| !value.is_empty());
            let empty = filament_type.is_none()
                && str_field(tray, "tray_info_idx")
                    .unwrap_or_default()
                    .is_empty();
            let absolute_slot = ams_id * 4 + tray_id;
            slots.push(AmsSlot {
                ams_id,
                tray_id,
                label: format!("{}", tray_index + 1),
                filament_type,
                sub_brand,
                color,
                remaining: u64_field(tray, "remain"),
                nozzle_min: f64_field(tray, "nozzle_temp_min"),
                nozzle_max: f64_field(tray, "nozzle_temp_max"),
                active: active == Some(absolute_slot),
                empty,
            });
        }
    }
    Some(slots)
}

fn parse_ams_humidity(print: &Value) -> Option<Vec<AmsHumidity>> {
    let units = print.get("ams")?.get("ams")?.as_array()?;
    let mut humidity = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        let ams_id = u64_field(unit, "id").unwrap_or(index as u64);
        if let Some(humidity_percent) = percent_humidity_field(unit) {
            humidity.push(AmsHumidity {
                ams_id,
                humidity_percent,
            });
        }
    }
    Some(humidity)
}

fn percent_humidity_field(unit: &Value) -> Option<u64> {
    // humidity_raw is the actual RH% on newer firmware
    for key in ["humidity_raw", "humidity_percent", "humidityPercent"] {
        if let Some(value) = u64_field(unit, key).filter(|value| (1..=100).contains(value)) {
            return Some(value);
        }
    }

    // Bambu's older AMS reports use low numeric humidity levels. Treat plain
    // `humidity` as RH% only when it cannot reasonably be a 1-5 level.
    u64_field(unit, "humidity").filter(|value| (6..=100).contains(value))
}

fn parse_alerts(print: &Value) -> Vec<PrinterAlert> {
    let mut alerts = Vec::new();
    if let Some(error_code) = str_field(print, "mc_print_error_code").filter(|value| value != "0") {
        alerts.push(PrinterAlert {
            code: error_code.clone(),
            level: AlertLevel::Error,
            message: format!("印刷エラー {error_code}"),
        });
    }
    if let Some(error_code) = u64_field(print, "print_error").filter(|value| *value != 0) {
        alerts.push(PrinterAlert {
            code: error_code.to_string(),
            level: AlertLevel::Error,
            message: format!("プリンターエラー {error_code}"),
        });
    }
    if let Some(items) = print.get("hms").and_then(Value::as_array) {
        for item in items {
            let code = str_field(item, "code")
                .or_else(|| u64_field(item, "code").map(|value| value.to_string()))
                .unwrap_or_else(|| "hms".to_string());
            alerts.push(PrinterAlert {
                code: code.clone(),
                level: AlertLevel::Warning,
                message: str_field(item, "msg").unwrap_or_else(|| format!("HMS {code}")),
            });
        }
    }
    alerts
}

fn map_status(state: &str) -> PrinterStatus {
    match state.to_ascii_uppercase().as_str() {
        "RUNNING" | "PRINTING" => PrinterStatus::Printing,
        "PREPARE" | "PREPARING" | "SLICING" => PrinterStatus::Preparing,
        "PAUSE" | "PAUSED" => PrinterStatus::Paused,
        "FINISH" | "FINISHED" | "SUCCESS" => PrinterStatus::Finished,
        "FAILED" | "FAIL" => PrinterStatus::Failed,
        "IDLE" => PrinterStatus::Idle,
        _ => PrinterStatus::Unknown,
    }
}

fn status_label(status: &PrinterStatus, raw: &str) -> &'static str {
    match status {
        PrinterStatus::Idle => "待機中",
        PrinterStatus::Connecting => "接続中",
        PrinterStatus::Preparing => "準備中",
        PrinterStatus::Printing => "印刷中",
        PrinterStatus::Paused => "一時停止",
        PrinterStatus::Finished => "完了",
        PrinterStatus::Failed => "エラー",
        PrinterStatus::Offline => "オフライン",
        PrinterStatus::Unknown if raw.is_empty() => "未取得",
        PrinterStatus::Unknown => "状態不明",
    }
}

fn normalize_color(value: &str) -> String {
    let trimmed = value.trim().trim_start_matches('#');
    if trimmed.len() >= 6 {
        format!("#{}", &trimmed[..6])
    } else {
        "#2a2f31".to_string()
    }
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|field| match field {
        Value::String(value) => Some(value.trim().to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|field| match field {
        Value::Number(value) => value.as_u64(),
        Value::String(value) => value.trim().parse::<u64>().ok(),
        _ => None,
    })
}

fn f64_field(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|field| match field {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.trim().parse::<f64>().ok(),
        _ => None,
    })
}

fn f64_field_any(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| f64_field(value, key))
}

fn f64_field_deep_any(value: &Value, keys: &[&str]) -> Option<f64> {
    f64_field_any(value, keys).or_else(|| match value {
        Value::Object(map) => map
            .values()
            .filter(|child| matches!(child, Value::Object(_) | Value::Array(_)))
            .find_map(|child| f64_field_deep_any(child, keys)),
        Value::Array(items) => items
            .iter()
            .filter(|child| matches!(child, Value::Object(_) | Value::Array(_)))
            .find_map(|child| f64_field_deep_any(child, keys)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::merge_report;
    use crate::types::{ConnectionMode, PrinterProfile, PrinterStatus};

    fn profile() -> PrinterProfile {
        PrinterProfile {
            id: "profile-1".to_string(),
            name: "P1S".to_string(),
            mode: ConnectionMode::Lan,
            ip: Some("192.168.1.10".to_string()),
            serial: Some("serial".to_string()),
            cloud_account: None,
            cloud_device_id: None,
            model: None,
            product: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn merges_partial_reports_without_losing_previous_values() {
        let profile = profile();
        let initial = merge_report(
            &profile,
            None,
            &json!({
                "print": {
                    "gcode_state": "RUNNING",
                    "mc_percent": 42,
                    "mc_remaining_time": 18,
                    "subtask_name": "drawer.3mf",
                    "ams": {
                        "tray_now": "1",
                        "ams": [{
                            "id": "0",
                            "tray": [
                                { "id": "0" },
                                { "id": "1", "tray_type": "PLA", "tray_color": "DFE2E3FF", "remain": 55 }
                            ]
                        }]
                    }
                }
            }),
        );

        let updated = merge_report(
            &profile,
            Some(initial),
            &json!({
                "print": {
                    "mc_percent": 43,
                    "nozzle_temper": 219.5
                }
            }),
        );

        assert_eq!(updated.progress, 43);
        assert_eq!(updated.remaining_minutes, Some(18));
        assert_eq!(updated.chamber_temp, None);
        assert_eq!(updated.job_name.as_deref(), Some("drawer.3mf"));
        assert_eq!(updated.nozzle_temp, Some(219.5));
        assert!(updated
            .ams_slots
            .iter()
            .any(|slot| slot.active && slot.tray_id == 1));
    }

    #[test]
    fn error_report_overrides_status() {
        let profile = profile();
        let snapshot = merge_report(
            &profile,
            None,
            &json!({
                "print": {
                    "gcode_state": "RUNNING",
                    "print_error": 123
                }
            }),
        );

        assert!(matches!(snapshot.status, PrinterStatus::Failed));
        assert_eq!(snapshot.alerts.len(), 1);
    }

    #[test]
    fn captures_chamber_temperature_and_percent_humidity() {
        let profile = profile();
        let snapshot = merge_report(
            &profile,
            None,
            &json!({
                "print": {
                    "chamber_temp": 38.4,
                    "ams": {
                        "ams": [
                            { "id": "0", "humidity_percent": 18, "tray": [] },
                            { "id": "1", "humidity": 46, "tray": [] },
                            { "id": "2", "humidityPercent": 101, "tray": [] }
                        ]
                    }
                }
            }),
        );

        assert_eq!(snapshot.chamber_temp, Some(38.4));
        assert_eq!(snapshot.ams_humidity.len(), 2);
        assert_eq!(snapshot.ams_humidity[0].humidity_percent, 18);
        assert_eq!(snapshot.ams_humidity[1].humidity_percent, 46);
    }

    #[test]
    fn ignores_ams_humidity_level_values() {
        let profile = profile();
        let snapshot = merge_report(
            &profile,
            None,
            &json!({
                "print": {
                    "ams": {
                        "ams": [
                            { "id": "0", "humidity": 3, "tray": [] },
                            { "id": "1", "humidity": "5", "tray": [] }
                        ]
                    }
                }
            }),
        );

        assert!(snapshot.ams_humidity.is_empty());
    }
}
