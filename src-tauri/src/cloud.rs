use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::{CloudLoginDevice, CloudLoginInput};

const API_BASE: &str = "https://api.bambulab.com";

#[derive(Debug, Clone)]
pub struct PendingCloudSession {
    pub id: String,
    pub account: String,
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
}

pub struct CloudClient {
    client: reqwest::Client,
}

pub struct LoginOutcome {
    pub requires_code: bool,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserPreference {
    pub uid: String,
}

impl CloudClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn login(&self, input: &CloudLoginInput) -> anyhow::Result<LoginOutcome> {
        let account = input.account.trim();
        let password = input
            .password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let code = input
            .code
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let body = LoginRequest {
            account,
            password,
            code,
            api_error: "",
        };
        let response = self
            .client
            .post(format!("{API_BASE}/v1/user-service/user/login"))
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let raw = response.text().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!("Cloud login failed: HTTP {status}: {raw}"));
        }
        let login: LoginResponse = serde_json::from_str(&raw).map_err(|error| {
            anyhow::anyhow!(
                "Cloud login failed: invalid JSON: {error}: {}",
                response_excerpt(&raw)
            )
        })?;
        let access_token = non_empty(login.access_token);
        let refresh_token = non_empty(login.refresh_token);
        let requires_code = login
            .login_type
            .as_deref()
            .is_some_and(|login_type| login_type.eq_ignore_ascii_case("verifyCode"))
            && access_token.is_none();
        let mut message = login.message;
        if requires_code && code.is_none() {
            self.send_verification_code(account).await?;
            let target = if account.contains('@') {
                "メール"
            } else {
                "SMS"
            };
            message = Some(format!(
                "認証コードを送信しました。{target}のコードを入力してください。"
            ));
        }
        Ok(LoginOutcome {
            requires_code,
            access_token,
            refresh_token,
            message,
        })
    }

    pub async fn preference(&self, token: &str) -> anyhow::Result<UserPreference> {
        let response = self
            .client
            .get(format!("{API_BASE}/v1/design-user-service/my/preference"))
            .headers(auth_headers(token)?)
            .send()
            .await?;
        let status = response.status();
        let raw = response.text().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Cloud user preference failed: HTTP {status}: {}",
                response_excerpt(&raw)
            ));
        }

        parse_user_preference(&raw)
    }

    pub async fn bound_devices(&self, token: &str) -> anyhow::Result<Vec<CloudLoginDevice>> {
        let response = self
            .client
            .get(format!("{API_BASE}/v1/iot-service/api/user/bind"))
            .headers(auth_headers(token)?)
            .send()
            .await?;
        let status = response.status();
        let raw = response.text().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Cloud device list failed: HTTP {status}: {}",
                response_excerpt(&raw)
            ));
        }

        parse_bound_devices(&raw)
    }

    pub async fn request_verification_code(&self, account: &str) -> anyhow::Result<()> {
        self.send_verification_code(account).await
    }

    async fn send_verification_code(&self, account: &str) -> anyhow::Result<()> {
        let (path, body) = if account.contains('@') {
            (
                "/v1/user-service/user/sendemail/code",
                serde_json::json!({
                    "email": account,
                    "type": "codeLogin",
                }),
            )
        } else {
            (
                "/v1/user-service/user/sendsmscode",
                serde_json::json!({
                    "phone": account,
                    "type": "codeLogin",
                }),
            )
        };
        let response = self
            .client
            .post(format!("{API_BASE}{path}"))
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let raw = response.text().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Cloud verification code request failed: HTTP {status}: {raw}"
            ));
        }
        if raw.trim().is_empty() {
            return Ok(());
        }

        let response: SendCodeResponse = serde_json::from_str(&raw)?;
        if response.success.is_some_and(|success| !success)
            || response
                .message
                .as_deref()
                .is_some_and(|message| message.eq_ignore_ascii_case("fail"))
        {
            return Err(anyhow::anyhow!(
                "Cloud verification code request failed: {}",
                response
                    .message
                    .unwrap_or_else(|| "unknown error".to_string())
            ));
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    account: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'a str>,
    #[serde(rename = "apiError")]
    api_error: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    login_type: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct SendCodeResponse {
    #[serde(default)]
    success: Option<bool>,
    #[serde(default)]
    message: Option<String>,
}

fn auth_headers(token: &str) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))?,
    );
    Ok(headers)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn field_string(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(value_to_string))
}

fn parse_user_preference(raw: &str) -> anyhow::Result<UserPreference> {
    let response: Value = serde_json::from_str(raw).map_err(|error| {
        anyhow::anyhow!(
            "Cloud user preference failed: invalid JSON: {error}: {}",
            response_excerpt(raw)
        )
    })?;
    let uid = field_string(&response, &["uid", "userId", "user_id"]).ok_or_else(|| {
        anyhow::anyhow!(
            "Cloud user preference failed: uid field is missing: {}",
            response_excerpt(raw)
        )
    })?;
    Ok(UserPreference { uid })
}

fn parse_bound_devices(raw: &str) -> anyhow::Result<Vec<CloudLoginDevice>> {
    let response: Value = serde_json::from_str(raw).map_err(|error| {
        anyhow::anyhow!(
            "Cloud device list failed: invalid JSON: {error}: {}",
            response_excerpt(raw)
        )
    })?;
    if let Some(message) = field_string(&response, &["message"])
        .filter(|message| !message.eq_ignore_ascii_case("success"))
    {
        return Err(anyhow::anyhow!("Cloud device list failed: {message}"));
    }

    let devices = response
        .get("devices")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Cloud device list failed: devices field is missing or not an array: {}",
                response_excerpt(raw)
            )
        })?
        .iter()
        .filter_map(|device| {
            let dev_id = field_string(device, &["dev_id", "devId", "device_id", "deviceId"])?;
            let name = field_string(device, &["name", "dev_name", "devName"])
                .unwrap_or_else(|| dev_id.clone());
            Some(CloudLoginDevice {
                dev_id,
                name,
                online: field_bool(device, &["online", "dev_online", "devOnline"]).unwrap_or(false),
                print_status: field_string(
                    device,
                    &["print_status", "printStatus", "task_status", "taskStatus"],
                ),
                model_name: field_string(
                    device,
                    &["dev_model_name", "devModelName", "model_name", "modelName"],
                ),
                product_name: field_string(
                    device,
                    &[
                        "dev_product_name",
                        "devProductName",
                        "product_name",
                        "productName",
                    ],
                ),
            })
        })
        .collect();
    Ok(devices)
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => non_empty(Some(value.clone())),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn field_bool(value: &Value, names: &[&str]) -> Option<bool> {
    names.iter().find_map(|name| {
        let value = value.get(*name)?;
        match value {
            Value::Bool(value) => Some(*value),
            Value::Number(value) => value.as_i64().map(|value| value != 0),
            Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "online" => Some(true),
                "false" | "0" | "no" | "offline" => Some(false),
                _ => None,
            },
            _ => None,
        }
    })
}

fn response_excerpt(raw: &str) -> String {
    const MAX_CHARS: usize = 300;
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= MAX_CHARS {
        compact
    } else {
        format!("{}...", compact.chars().take(MAX_CHARS).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_bound_devices, parse_user_preference};

    #[test]
    fn parses_user_preference_uid_as_string() {
        let preference = parse_user_preference(r#"{"uid":"12345"}"#).unwrap();

        assert_eq!(preference.uid, "12345");
    }

    #[test]
    fn parses_device_list_with_mixed_field_types() {
        let devices = parse_bound_devices(
            r#"{
              "message": "success",
              "devices": [
                {
                  "devId": 123,
                  "devName": "P1S",
                  "devOnline": "online",
                  "printStatus": 2,
                  "modelName": "C12",
                  "productName": "Bambu P1S"
                }
              ]
            }"#,
        )
        .unwrap();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].dev_id, "123");
        assert_eq!(devices[0].name, "P1S");
        assert!(devices[0].online);
        assert_eq!(devices[0].print_status.as_deref(), Some("2"));
        assert_eq!(devices[0].model_name.as_deref(), Some("C12"));
    }
}
