use std::{sync::Arc, time::Duration};

use native_tls::TlsConnector;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, TlsConfiguration, Transport};
use serde_json::json;
use tauri::Emitter;
use tokio::{sync::RwLock, time};

use crate::{
    normalize::merge_report,
    types::{ConnectionMode, PrinterProfile, PrinterSnapshot, ProfileSecret},
    RuntimeState,
};

#[derive(Clone)]
pub struct MqttConnectionConfig {
    pub mode: ConnectionMode,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub device_id: String,
    pub client_id: String,
}

impl MqttConnectionConfig {
    pub fn from_profile(profile: &PrinterProfile, secret: ProfileSecret) -> Result<Self, String> {
        match profile.mode {
            ConnectionMode::Lan => {
                let host = profile
                    .ip
                    .clone()
                    .ok_or_else(|| "LAN profile is missing printer IP.".to_string())?;
                let device_id = profile
                    .serial
                    .clone()
                    .ok_or_else(|| "LAN profile is missing serial / device id.".to_string())?;
                let password = secret
                    .lan_access_code
                    .ok_or_else(|| "LAN access code is missing from OS credentials.".to_string())?;
                Ok(Self {
                    mode: ConnectionMode::Lan,
                    host,
                    port: 8883,
                    username: "bblp".to_string(),
                    password,
                    client_id: device_id.clone(),
                    device_id,
                })
            }
            ConnectionMode::Cloud => {
                let device_id = profile
                    .cloud_device_id
                    .clone()
                    .ok_or_else(|| "Cloud profile is missing device id.".to_string())?;
                let user_id = secret
                    .cloud_user_id
                    .ok_or_else(|| "Cloud user id is missing from OS credentials.".to_string())?;
                let password = secret
                    .cloud_access_token
                    .ok_or_else(|| "Cloud token is missing from OS credentials.".to_string())?;
                Ok(Self {
                    mode: ConnectionMode::Cloud,
                    host: "us.mqtt.bambulab.com".to_string(),
                    port: 8883,
                    username: format!("u_{user_id}"),
                    password,
                    client_id: format!("bambu-overlay-{device_id}"),
                    device_id,
                })
            }
        }
    }
}

pub async fn start_mqtt_task(
    config: MqttConnectionConfig,
    profile: PrinterProfile,
    runtime: Arc<RwLock<RuntimeState>>,
    app: tauri::AppHandle,
) {
    let mut backoff_secs = 2u64;
    loop {
        let result = run_mqtt(
            config.clone(),
            profile.clone(),
            runtime.clone(),
            app.clone(),
        )
        .await;
        if let Err(error) = result {
            let snapshot = PrinterSnapshot::offline(&profile, &format!("接続失敗: {error}"));
            {
                let mut rt = runtime.write().await;
                rt.snapshot = Some(snapshot.clone());
            }
            let _ = app.emit("snapshot-updated", &snapshot);
        }
        // 指数バックオフで再接続（abort されたらここで task がキャンセルされる）
        time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(30);

        let reconnecting = PrinterSnapshot::connecting(&profile);
        {
            let mut rt = runtime.write().await;
            rt.snapshot = Some(reconnecting.clone());
        }
        let _ = app.emit("snapshot-updated", &reconnecting);
    }
}

async fn run_mqtt(
    config: MqttConnectionConfig,
    profile: PrinterProfile,
    runtime: Arc<RwLock<RuntimeState>>,
    app: tauri::AppHandle,
) -> anyhow::Result<()> {
    let mut options = MqttOptions::new(config.client_id.clone(), config.host.clone(), config.port);
    options.set_keep_alive(Duration::from_secs(20));
    options.set_credentials(config.username.clone(), config.password.clone());
    options.set_transport(match config.mode {
        ConnectionMode::Cloud => Transport::tls_with_config(TlsConfiguration::Native),
        ConnectionMode::Lan => {
            let connector = TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true)
                .build()?;
            Transport::tls_with_config(TlsConfiguration::NativeConnector(connector))
        }
    });

    let report_topic = format!("device/{}/report", config.device_id);
    let request_topic = format!("device/{}/request", config.device_id);
    let (client, mut event_loop) = AsyncClient::new(options, 32);
    client.subscribe(report_topic, QoS::AtLeastOnce).await?;
    publish_push_all(&client, &request_topic).await?;

    let mut push_timer = time::interval(Duration::from_secs(300));
    loop {
        tokio::select! {
            _ = push_timer.tick() => {
                publish_push_all(&client, &request_topic).await?;
            }
            event = event_loop.poll() => {
                match event? {
                    Event::Incoming(Packet::Publish(publish)) => {
                        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&publish.payload) {
                            let current = runtime.read().await.snapshot.clone();
                            let snapshot = merge_report(&profile, current, &value);
                            let useful_diagnostic = value.get("print")
                                .map(|p| {
                                    p.get("ams").is_some()
                                        || p.get("nozzle_temper").is_some()
                                        || p.get("bed_temper").is_some()
                                })
                                .unwrap_or(false);
                            let mut rt = runtime.write().await;
                            if useful_diagnostic {
                                rt.last_raw_payload = serde_json::to_string_pretty(&value).ok();
                            }
                            rt.snapshot = Some(snapshot.clone());
                            drop(rt);
                            let _ = app.emit("snapshot-updated", &snapshot);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn publish_push_all(client: &AsyncClient, request_topic: &str) -> anyhow::Result<()> {
    let sequence_id = chrono::Utc::now().timestamp_millis().to_string();
    let payload = json!({
        "pushing": {
            "sequence_id": sequence_id,
            "command": "pushall",
            "version": 1,
            "push_target": 1
        }
    });
    client
        .publish(
            request_topic,
            QoS::AtLeastOnce,
            false,
            serde_json::to_vec(&payload)?,
        )
        .await?;
    Ok(())
}
