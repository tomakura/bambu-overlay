mod cloud;
mod mqtt;
mod normalize;
mod storage;
mod types;

use std::sync::Arc;

use cloud::{CloudClient, PendingCloudSession};
use mqtt::{start_mqtt_task, MqttConnectionConfig};
use storage::{is_missing_credential, load_store, save_store, CredentialStore};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, LogicalSize, Manager, Size,
};
use tokio::sync::RwLock;
use types::{
    AppBootstrap, CloudLoginInput, CloudLoginResult, ConnectionMode, OverlayPreferences,
    PrinterProfile, PrinterSnapshot, SaveCloudProfileInput, SaveLanProfileInput, StoredStore,
};
use uuid::Uuid;

const MENU_SHOW: &str = "show-window";
const MENU_DISABLE_CLICK_THROUGH: &str = "disable-click-through";
const MENU_QUIT: &str = "quit";
const NORMAL_WINDOW_WIDTH: f64 = 430.0;
const NORMAL_WINDOW_HEIGHT_BASE: f64 = 500.0;
const NORMAL_COLLAPSED_WINDOW_HEIGHT: f64 = 332.0;
const SETTINGS_WINDOW_WIDTH: f64 = 430.0;
const SETTINGS_WINDOW_HEIGHT: f64 = 560.0;
const COMPACT_WINDOW_WIDTH: f64 = 560.0;
const COMPACT_WINDOW_HEIGHT: f64 = 208.0;
const COMPACT_EXPANDED_WINDOW_HEIGHT: f64 = 284.0;

#[derive(Default)]
struct RuntimeState {
    store: StoredStore,
    snapshot: Option<PrinterSnapshot>,
    pending_cloud: Option<PendingCloudSession>,
    connection_task: Option<tauri::async_runtime::JoinHandle<()>>,
    last_raw_payload: Option<String>,
}

struct AppState {
    runtime: Arc<RwLock<RuntimeState>>,
    credentials: CredentialStore,
}

#[tauri::command]
async fn get_app_bootstrap(state: tauri::State<'_, AppState>) -> Result<AppBootstrap, String> {
    let runtime = state.runtime.read().await;
    Ok(AppBootstrap {
        profiles: runtime.store.profiles.clone(),
        selected_profile_id: runtime.store.selected_profile_id.clone(),
        snapshot: runtime.snapshot.clone(),
        preferences: runtime.store.preferences.clone(),
    })
}

#[tauri::command]
async fn save_lan_profile(
    input: SaveLanProfileInput,
    state: tauri::State<'_, AppState>,
) -> Result<PrinterProfile, String> {
    input.validate()?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut runtime = state.runtime.write().await;
    let serial = input.serial.trim().to_owned();
    let existing = runtime
        .store
        .profiles
        .iter()
        .find(|profile| {
            matches!(profile.mode, ConnectionMode::Lan)
                && profile.serial.as_deref() == Some(serial.as_str())
        })
        .cloned();
    let profile = PrinterProfile {
        id: existing
            .as_ref()
            .map(|profile| profile.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: input.name.trim().to_owned(),
        mode: ConnectionMode::Lan,
        ip: Some(input.ip.trim().to_owned()),
        serial: Some(serial),
        cloud_account: None,
        cloud_device_id: None,
        model: None,
        product: None,
        created_at: existing
            .as_ref()
            .map(|profile| profile.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    state
        .credentials
        .save_profile_secret(
            &profile.id,
            &types::ProfileSecret::lan(input.access_code.trim().to_owned()),
        )
        .map_err(|error| error.to_string())?;
    runtime.store.selected_profile_id = Some(profile.id.clone());
    if let Some(index) = runtime
        .store
        .profiles
        .iter()
        .position(|stored| stored.id == profile.id)
    {
        runtime.store.profiles[index] = profile.clone();
    } else {
        runtime.store.profiles.push(profile.clone());
    }
    save_store(&runtime.store).map_err(|error| error.to_string())?;
    Ok(profile)
}

#[tauri::command]
async fn cloud_login(
    input: CloudLoginInput,
    state: tauri::State<'_, AppState>,
) -> Result<CloudLoginResult, String> {
    input.validate()?;
    let client = CloudClient::new();
    let login = client
        .login(&input)
        .await
        .map_err(|error| format_cloud_error("Cloud login failed", error))?;
    if login.requires_code {
        return Ok(CloudLoginResult {
            requires_code: true,
            session_id: None,
            devices: vec![],
            message: login.message,
        });
    }

    let token = login.access_token.ok_or_else(|| {
        "Cloud認証に失敗しました。認証コードを入力して再試行してください。".to_string()
    })?;
    let user = match client.preference(&token).await {
        Ok(user) => user,
        Err(error) if is_cloud_unauthorized(&error) => {
            return handle_cloud_reauth(&client, &input).await;
        }
        Err(error) => return Err(format_cloud_error("Cloud user preference failed", error)),
    };
    let devices = match client.bound_devices(&token).await {
        Ok(devices) => devices,
        Err(error) if is_cloud_unauthorized(&error) => {
            return handle_cloud_reauth(&client, &input).await;
        }
        Err(error) => return Err(format_cloud_error("Cloud device list failed", error)),
    };
    let session = PendingCloudSession {
        id: Uuid::new_v4().to_string(),
        account: input.account.trim().to_owned(),
        user_id: user.uid,
        access_token: token,
        refresh_token: login.refresh_token,
    };
    let session_id = session.id.clone();
    state.runtime.write().await.pending_cloud = Some(session);
    Ok(CloudLoginResult {
        requires_code: false,
        session_id: Some(session_id),
        devices,
        message: None,
    })
}

#[tauri::command]
async fn save_cloud_profile(
    input: SaveCloudProfileInput,
    state: tauri::State<'_, AppState>,
) -> Result<PrinterProfile, String> {
    input.validate()?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut runtime = state.runtime.write().await;
    let session = runtime
        .pending_cloud
        .clone()
        .filter(|session| session.id == input.session_id)
        .ok_or_else(|| "Cloud login session expired. Log in again.".to_string())?;
    let device_id = input.device_id.trim().to_owned();
    let existing = runtime
        .store
        .profiles
        .iter()
        .find(|profile| {
            matches!(profile.mode, ConnectionMode::Cloud)
                && profile.cloud_device_id.as_deref() == Some(device_id.as_str())
        })
        .cloned();

    let profile = PrinterProfile {
        id: existing
            .as_ref()
            .map(|profile| profile.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: input.name.trim().to_owned(),
        mode: ConnectionMode::Cloud,
        ip: None,
        serial: None,
        cloud_account: Some(session.account.clone()),
        cloud_device_id: Some(device_id),
        model: existing.as_ref().and_then(|profile| profile.model.clone()),
        product: existing
            .as_ref()
            .and_then(|profile| profile.product.clone()),
        created_at: existing
            .as_ref()
            .map(|profile| profile.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };

    state
        .credentials
        .save_profile_secret(
            &profile.id,
            &types::ProfileSecret::cloud(
                session.user_id,
                session.access_token,
                session.refresh_token,
            ),
        )
        .map_err(|error| error.to_string())?;
    runtime.store.selected_profile_id = Some(profile.id.clone());
    if let Some(index) = runtime
        .store
        .profiles
        .iter()
        .position(|stored| stored.id == profile.id)
    {
        runtime.store.profiles[index] = profile.clone();
    } else {
        runtime.store.profiles.push(profile.clone());
    }
    runtime.pending_cloud = None;
    save_store(&runtime.store).map_err(|error| error.to_string())?;
    Ok(profile)
}

#[tauri::command]
async fn connect_profile(
    profile_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<PrinterSnapshot, String> {
    let (profile, preferences) = {
        let runtime = state.runtime.read().await;
        let profile = runtime
            .store
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
            .ok_or_else(|| "Profile not found.".to_string())?;
        (profile, runtime.store.preferences.clone())
    };
    let secret = state
        .credentials
        .load_profile_secret(&profile.id)
        .map_err(|error| format_profile_secret_error(&profile, error))?;

    {
        let mut runtime = state.runtime.write().await;
        if !runtime
            .store
            .profiles
            .iter()
            .any(|profile| profile.id == profile_id)
        {
            return Err("Profile not found.".to_string());
        }
        runtime.store.selected_profile_id = Some(profile_id.clone());
        save_store(&runtime.store).map_err(|error| error.to_string())?;
    }

    disconnect_current(&state).await;
    apply_window_preferences(&app, &preferences)?;

    let initial_snapshot = PrinterSnapshot::connecting(&profile);
    {
        let mut runtime = state.runtime.write().await;
        runtime.snapshot = Some(initial_snapshot.clone());
    }
    let _ = app.emit("snapshot-updated", &initial_snapshot);

    let config = MqttConnectionConfig::from_profile(&profile, secret)?;
    let runtime = state.runtime.clone();
    let app_for_task = app.clone();
    let profile_for_task = profile.clone();
    let handle = tauri::async_runtime::spawn(async move {
        start_mqtt_task(config, profile_for_task, runtime, app_for_task).await;
    });
    state.runtime.write().await.connection_task = Some(handle);
    Ok(initial_snapshot)
}

#[tauri::command]
async fn delete_profile(
    profile_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut runtime = state.runtime.write().await;
    let before = runtime.store.profiles.len();
    runtime
        .store
        .profiles
        .retain(|profile| profile.id != profile_id);
    if runtime.store.profiles.len() == before {
        return Err("Profile not found.".to_string());
    }

    if runtime.store.selected_profile_id.as_deref() == Some(&profile_id) {
        runtime.store.selected_profile_id = runtime
            .store
            .profiles
            .first()
            .map(|profile| profile.id.clone());
        if let Some(handle) = runtime.connection_task.take() {
            handle.abort();
        }
    }
    if runtime
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.profile_id.as_deref())
        == Some(&profile_id)
    {
        runtime.snapshot = None;
    }
    save_store(&runtime.store).map_err(|error| error.to_string())?;
    drop(runtime);
    state
        .credentials
        .delete_profile_secret(&profile_id)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
async fn disconnect_profile(state: tauri::State<'_, AppState>) -> Result<(), String> {
    disconnect_current(&state).await;
    Ok(())
}

#[tauri::command]
async fn set_overlay_preferences(
    preferences: OverlayPreferences,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    preferences.validate()?;
    apply_window_preferences(&app, &preferences)?;
    let mut runtime = state.runtime.write().await;
    runtime.store.preferences = preferences;
    save_store(&runtime.store).map_err(|error| error.to_string())
}

#[tauri::command]
fn move_window_by(delta_x: i32, delta_y: i32, app: tauri::AppHandle) -> Result<(), String> {
    if delta_x == 0 && delta_y == 0 {
        return Ok(());
    }

    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found.".to_string())?;
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let delta_x = (delta_x as f64 * scale).round() as i32;
    let delta_y = (delta_y as f64 * scale).round() as i32;
    window
        .set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            position.x + delta_x,
            position.y + delta_y,
        )))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn close_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found.".to_string())?;
    window.close().map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_last_raw_payload(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.runtime.read().await.last_raw_payload.clone())
}

fn normal_expanded_height(alerts_count: usize) -> f64 {
    if alerts_count == 0 {
        return NORMAL_WINDOW_HEIGHT_BASE;
    }
    let alerts_height = 8.0
        + (alerts_count as f64 * 40.0)
        + (alerts_count.saturating_sub(1) as f64 * 6.0);
    (NORMAL_WINDOW_HEIGHT_BASE + alerts_height).min(650.0)
}

#[tauri::command]
fn set_window_layout(
    compact: bool,
    expanded: bool,
    settings_open: bool,
    alerts_count: usize,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let (width, height) = if settings_open {
            if compact {
                (COMPACT_WINDOW_WIDTH, COMPACT_EXPANDED_WINDOW_HEIGHT)
            } else {
                (SETTINGS_WINDOW_WIDTH, SETTINGS_WINDOW_HEIGHT)
            }
        } else if compact && expanded {
            (COMPACT_WINDOW_WIDTH, COMPACT_EXPANDED_WINDOW_HEIGHT)
        } else if compact {
            (COMPACT_WINDOW_WIDTH, COMPACT_WINDOW_HEIGHT)
        } else if expanded {
            (NORMAL_WINDOW_WIDTH, normal_expanded_height(alerts_count))
        } else {
            (NORMAL_WINDOW_WIDTH, NORMAL_COLLAPSED_WINDOW_HEIGHT)
        };
        window
            .set_size(Size::Logical(LogicalSize::new(width, height)))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn disconnect_current(state: &tauri::State<'_, AppState>) {
    if let Some(handle) = state.runtime.write().await.connection_task.take() {
        handle.abort();
    }
}

fn apply_window_preferences(
    app: &tauri::AppHandle,
    preferences: &OverlayPreferences,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_shadow(false);
        window
            .set_always_on_top(preferences.always_on_top)
            .map_err(|error| error.to_string())?;
        window
            .set_ignore_cursor_events(preferences.click_through)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, MENU_SHOW, "表示", true, None::<&str>)?;
    let disable_click_through = MenuItem::with_id(
        app,
        MENU_DISABLE_CLICK_THROUGH,
        "クリック透過を解除",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &disable_click_through, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Bambu Overlay")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_SHOW | MENU_DISABLE_CLICK_THROUGH => force_click_through_off(app),
            MENU_QUIT => app.exit(0),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

fn force_click_through_off(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_ignore_cursor_events(false);
        let _ = window.show();
        let _ = window.set_focus();
    }

    let runtime = app.state::<AppState>().runtime.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let preferences = {
            let mut runtime = runtime.write().await;
            runtime.store.preferences.click_through = false;
            let preferences = runtime.store.preferences.clone();
            let _ = save_store(&runtime.store);
            preferences
        };
        let _ = app.emit("preferences-updated", &preferences);
    });
}

fn format_cloud_error(action: &str, error: anyhow::Error) -> String {
    if is_cloud_unauthorized(&error) {
        return "Cloud認証に失敗しました。Bambu Lab側で認証コードが必要、またはトークンが無効です。認証コードを入力して再試行してください。"
            .to_string();
    }
    let message = error.to_string();
    if message.starts_with(action) {
        message
    } else {
        format!("{action}: {message}")
    }
}

fn format_profile_secret_error(profile: &PrinterProfile, error: anyhow::Error) -> String {
    if is_missing_credential(&error) {
        format!(
            "{} の認証情報がWindowsの安全な保存領域に見つかりません。設定でこのプロファイルを削除して、再登録してください。",
            profile.name
        )
    } else {
        format!("{} の認証情報を読み込めませんでした: {error}", profile.name)
    }
}

async fn handle_cloud_reauth(
    client: &CloudClient,
    input: &CloudLoginInput,
) -> Result<CloudLoginResult, String> {
    if input
        .code
        .as_deref()
        .is_some_and(|code| !code.trim().is_empty())
    {
        return Ok(CloudLoginResult {
            requires_code: true,
            session_id: None,
            devices: vec![],
            message: Some(
                "認証コードを確認できませんでした。コードを確認して再入力してください。"
                    .to_string(),
            ),
        });
    }

    client
        .request_verification_code(input.account.trim())
        .await
        .map_err(|error| format_cloud_error("Cloud verification code request failed", error))?;
    Ok(CloudLoginResult {
        requires_code: true,
        session_id: None,
        devices: vec![],
        message: Some(format!(
            "認証コードを送信しました。{}のコードを入力してください。",
            verification_target(&input.account)
        )),
    })
}

fn is_cloud_unauthorized(error: &anyhow::Error) -> bool {
    let reqwest_status = error
        .downcast_ref::<reqwest::Error>()
        .and_then(|error| error.status());
    reqwest_status == Some(reqwest::StatusCode::UNAUTHORIZED)
        || error.to_string().contains("HTTP 401")
        || error.to_string().contains("401 Unauthorized")
}

fn verification_target(account: &str) -> &'static str {
    if account.contains('@') {
        "メール"
    } else {
        "SMS"
    }
}

pub fn run() {
    let mut store = load_store().unwrap_or_default();
    if store.preferences.click_through {
        store.preferences.click_through = false;
        let _ = save_store(&store);
    }
    let state = AppState {
        runtime: Arc::new(RwLock::new(RuntimeState {
            store,
            snapshot: None,
            pending_cloud: None,
            connection_task: None,
            last_raw_payload: None,
        })),
        credentials: CredentialStore::new(),
    };

    tauri::Builder::default()
        .manage(state)
        .setup(|app| {
            let app_handle = app.handle().clone();
            setup_tray(&app_handle)?;
            let state = app.state::<AppState>();
            let preferences = tauri::async_runtime::block_on(async {
                state.runtime.read().await.store.preferences.clone()
            });
            let _ = apply_window_preferences(&app_handle, &preferences);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_bootstrap,
            save_lan_profile,
            cloud_login,
            save_cloud_profile,
            connect_profile,
            delete_profile,
            disconnect_profile,
            set_overlay_preferences,
            move_window_by,
            close_window,
            set_window_layout,
            get_last_raw_payload
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bambu Overlay");
}
