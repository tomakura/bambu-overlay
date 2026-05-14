use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StoredStore {
    pub profiles: Vec<PrinterProfile>,
    pub selected_profile_id: Option<String>,
    #[serde(default)]
    pub preferences: OverlayPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrap {
    pub profiles: Vec<PrinterProfile>,
    pub selected_profile_id: Option<String>,
    pub snapshot: Option<PrinterSnapshot>,
    pub preferences: OverlayPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayPreferences {
    pub always_on_top: bool,
    pub click_through: bool,
    pub opacity: f64,
    pub compact: bool,
}

impl Default for OverlayPreferences {
    fn default() -> Self {
        Self {
            always_on_top: true,
            click_through: false,
            opacity: 0.94,
            compact: false,
        }
    }
}

impl OverlayPreferences {
    pub fn validate(&self) -> Result<(), String> {
        if !(0.4..=1.0).contains(&self.opacity) {
            return Err("Opacity must be between 0.4 and 1.0.".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterProfile {
    pub id: String,
    pub name: String,
    pub mode: ConnectionMode,
    pub ip: Option<String>,
    pub serial: Option<String>,
    pub cloud_account: Option<String>,
    pub cloud_device_id: Option<String>,
    pub model: Option<String>,
    pub product: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionMode {
    Lan,
    Cloud,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrinterStatus {
    Unknown,
    Connecting,
    Idle,
    Preparing,
    Printing,
    Paused,
    Finished,
    Failed,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterSnapshot {
    pub profile_id: Option<String>,
    pub printer_name: String,
    pub mode: ConnectionMode,
    pub connected: bool,
    pub last_updated: Option<String>,
    pub status: PrinterStatus,
    pub raw_state: Option<String>,
    pub status_label: String,
    pub job_name: Option<String>,
    pub progress: u8,
    pub remaining_minutes: Option<u64>,
    pub current_layer: Option<u64>,
    pub total_layers: Option<u64>,
    pub nozzle_temp: Option<f64>,
    pub nozzle_target: Option<f64>,
    pub bed_temp: Option<f64>,
    pub bed_target: Option<f64>,
    pub chamber_temp: Option<f64>,
    pub chamber_target: Option<f64>,
    pub speed_percent: Option<u64>,
    pub speed_level: Option<u64>,
    pub wifi_signal: Option<String>,
    pub ams_humidity: Vec<AmsHumidity>,
    pub ams_slots: Vec<AmsSlot>,
    pub alerts: Vec<PrinterAlert>,
}

impl PrinterSnapshot {
    pub fn offline(profile: &PrinterProfile, label: &str) -> Self {
        Self {
            profile_id: Some(profile.id.clone()),
            printer_name: profile.name.clone(),
            mode: profile.mode.clone(),
            connected: false,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
            status: PrinterStatus::Offline,
            raw_state: None,
            status_label: label.to_string(),
            job_name: None,
            progress: 0,
            remaining_minutes: None,
            current_layer: None,
            total_layers: None,
            nozzle_temp: None,
            nozzle_target: None,
            bed_temp: None,
            bed_target: None,
            chamber_temp: None,
            chamber_target: None,
            speed_percent: None,
            speed_level: None,
            wifi_signal: None,
            ams_humidity: vec![],
            ams_slots: vec![],
            alerts: vec![],
        }
    }

    pub fn connecting(profile: &PrinterProfile) -> Self {
        let mut snapshot = Self::offline(profile, "接続中");
        snapshot.status = PrinterStatus::Connecting;
        snapshot
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmsHumidity {
    pub ams_id: u64,
    pub humidity_percent: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmsSlot {
    pub ams_id: u64,
    pub tray_id: u64,
    pub label: String,
    pub filament_type: Option<String>,
    pub sub_brand: Option<String>,
    pub color: String,
    pub remaining: Option<u64>,
    pub nozzle_min: Option<f64>,
    pub nozzle_max: Option<f64>,
    pub active: bool,
    pub empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterAlert {
    pub code: String,
    pub level: AlertLevel,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSecret {
    pub lan_access_code: Option<String>,
    pub cloud_user_id: Option<String>,
    pub cloud_access_token: Option<String>,
    pub cloud_refresh_token: Option<String>,
}

impl ProfileSecret {
    pub fn lan(access_code: String) -> Self {
        Self {
            lan_access_code: Some(access_code),
            cloud_user_id: None,
            cloud_access_token: None,
            cloud_refresh_token: None,
        }
    }

    pub fn cloud(user_id: String, access_token: String, refresh_token: Option<String>) -> Self {
        Self {
            lan_access_code: None,
            cloud_user_id: Some(user_id),
            cloud_access_token: Some(access_token),
            cloud_refresh_token: refresh_token,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveLanProfileInput {
    pub name: String,
    pub ip: String,
    pub serial: String,
    pub access_code: String,
}

impl SaveLanProfileInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Printer name is required.".to_string());
        }
        if self.ip.trim().is_empty() {
            return Err("Printer IP is required.".to_string());
        }
        if self.serial.trim().is_empty() {
            return Err("Serial / device id is required.".to_string());
        }
        if self.access_code.trim().is_empty() {
            return Err("LAN access code is required.".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudLoginInput {
    pub account: String,
    pub password: Option<String>,
    pub code: Option<String>,
}

impl CloudLoginInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.account.trim().is_empty() {
            return Err("Bambu account is required.".to_string());
        }
        if self
            .password
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
            && self.code.as_deref().unwrap_or_default().trim().is_empty()
        {
            return Err("Password or verification code is required.".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCloudProfileInput {
    pub session_id: String,
    pub device_id: String,
    pub name: String,
}

impl SaveCloudProfileInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.session_id.trim().is_empty() {
            return Err("Cloud session is missing.".to_string());
        }
        if self.device_id.trim().is_empty() {
            return Err("Cloud device id is required.".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("Printer name is required.".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudLoginDevice {
    pub dev_id: String,
    pub name: String,
    pub online: bool,
    pub print_status: Option<String>,
    pub model_name: Option<String>,
    pub product_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudLoginResult {
    pub requires_code: bool,
    pub session_id: Option<String>,
    pub devices: Vec<CloudLoginDevice>,
    pub message: Option<String>,
}
