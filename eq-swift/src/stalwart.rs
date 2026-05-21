//! Stalwart-Lite archive transport adapter.
//!
//! Communicates with a local `../stalwart-lite` server via JMAP HTTP APIs:
//! - `/.well-known/jmap` → discover the JMAP session URL
//! - `/jmap` → archive operations via JMAP Mail + Archive specs
//!
//! ## Configuration
//!
//! The adapter reads its config from environment variables (never stored in git):
//! - `STALWART_BASE_URL` — base URL of the Stalwart server (default: `http://localhost:8080`)
//! - `STALWART_USERNAME` — JMAP account username
//! - `STALWART_PASSWORD` — JMAP account password
//!
//! If credentials are missing the adapter reports `connected: false` but won't error.

use crate::transport_types::{
    envelope_err, envelope_ok, TransportHealth, TransportInfo, TransportMessage,
};

use serde_json::Value;

const TRANSPORT_ID: &str = "stalwart";

// ── StalwartArchiveClient ─────────────────────────────────────────────────────

pub struct StalwartClient {
    base_url: String,
    username: Option<String>,
    password: Option<String>,
    api_url: Option<String>,
    account_id: Option<String>,
}

impl StalwartClient {
    /// Read config from environment. Returns a client even if credentials
    /// are missing — it will just report `connected: false`.
    pub fn from_env() -> Self {
        let base_url = std::env::var("STALWART_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());

        Self {
            base_url,
            username: std::env::var("STALWART_USERNAME").ok(),
            password: std::env::var("STALWART_PASSWORD").ok(),
            api_url: None,
            account_id: None,
        }
    }

    /// Concrete config for testing / programmatic use.
    pub fn new(base_url: String, username: &str, password: &str) -> Self {
        Self {
            base_url,
            username: Some(username.to_string()),
            password: Some(password.to_string()),
            api_url: None,
            account_id: None,
        }
    }

    fn configured(&self) -> bool {
        self.username.is_some() && self.password.is_some()
    }

    /// Discover JMAP session: hit `/.well-known/jmap`, then the session endpoint.
    fn discover(&mut self) -> Result<(), String> {
        let well_known_url = format!("{}/.well-known/jmap", self.base_url.trim_end_matches('/'));

        let resp: Value = ureq::get(&well_known_url)
            .call()
            .map_err(|e| format!("JMAP discovery failed: {e}"))?
            .into_json()
            .map_err(|e| format!("JMAP discovery parse error: {e}"))?;

        let session_url = resp["apiUrl"]
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| {
                resp.get("href")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| format!("{}/jmap", self.base_url.trim_end_matches('/')));

        // Get session to find account ID — deserialize from raw JSON.
        let session: serde_json::Value = ureq::get(&session_url)
            .call()
            .map_err(|e| format!("JMAP session error: {e}"))?
            .into_json()
            .map_err(|e| format!("JMAP session parse error: {e}"))?;

        self.api_url = session["apiUrl"].as_str().map(|s| s.to_string());

        if let Some(account_val) = session["primaryAccounts"].get("urn:ietf:params:jmap:mail") {
            self.account_id = account_val.as_str().map(|s| s.to_string());
        }

        Ok(())
    }

    fn auth_header(&self) -> String {
        let user = self.username.as_deref().unwrap_or("");
        let pass = self.password.as_deref().unwrap_or("");
        let encoded = base64_encode(&format!("{user}:{pass}"));
        format!("Basic {encoded}")
    }

    fn jmap_call(&self, requests: &[Value]) -> Result<Value, String> {
        let api_url = self.api_url.as_ref().ok_or("JMAP session not discovered")?;

        let body = ureq::json!({
            "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"],
            "methodCalls": requests,
        });

        let resp: Value = ureq::post(api_url)
            .set("Authorization", &self.auth_header())
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| format!("JMAP request failed: {e}"))?
            .into_json()
            .map_err(|e| format!("JMAP response parse error: {e}"))?;

        Ok(resp)
    }

    fn jmap_echo(&mut self) -> Result<bool, String> {
        if self.api_url.is_none() {
            self.discover()?;
        }
        let resp = self.jmap_call(&[ureq::json!(["Core/echo", {}, "echo-1"])])?;
        Ok(resp["methodResponses"][0][1].is_object())
    }

    fn list_messages(&mut self) -> Result<Vec<TransportMessage>, String> {
        if self.api_url.is_none() {
            self.discover()?;
        }
        let resp = self.jmap_call(&[
            ureq::json!(["Email/query", { "accountId": self.account_id }, "query-1"]),
            ureq::json!([
                "Email/get",
                {
                    "accountId": self.account_id,
                    "#ids": { "resultOf": "query-1", "name": "Email/query", "path": "/ids/*" },
                    "properties": ["id", "subject", "sender", "receivedAt"]
                },
                "get-1"
            ]),
        ])?;

        let method_responses = &resp["methodResponses"];
        let list = &method_responses[1][1]["list"];
        let messages: Vec<TransportMessage> = list
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|email| {
                let subject = email["subject"]
                    .as_str()
                    .unwrap_or("(no subject)")
                    .to_string();
                TransportMessage {
                    id: email["id"].as_str().unwrap_or("?").to_string(),
                    text: subject,
                    transport: "stalwart".to_string(),
                    status: "archived".to_string(),
                    metadata: email.clone(),
                }
            })
            .collect();

        Ok(messages)
    }

    fn archive_message(&mut self, text: &str) -> Result<Value, String> {
        if self.api_url.is_none() {
            self.discover()?;
        }
        let account_id = self.account_id.as_deref().unwrap_or("");

        let resp = self.jmap_call(&[ureq::json!([
            "Email/import",
            {
                "accountId": account_id,
                "emails": {
                    "id": format!("msg-{}", timestamp()),
                    "mailboxIds": { "INBOX": true },
                    "subject": truncate(text, 78),
                    "bodyStructure": {
                        "type": "text/plain",
                        "text": text
                    },
                    "receivedAt": utc_now(),
                }
            },
            "import-1"
        ])])?;

        Ok(resp["methodResponses"][0][1].clone())
    }
}

impl StalwartClient {
    /// Same routing as Aurorality's Stalwart `NativePlugin::invoke`.
    pub fn invoke(&self, method: &str, payload: &Value) -> Result<Value, String> {
        // StalwartClient methods internally mutate session state (api_url, account_id).
        // We use interior mutability: each call borrows mutably via a fresh client clone.
        // In practice the bridge holds the plugin behind Arc<RwLock<dyn NativePlugin>>,
        // so the RwLock write guard provides unique access.
        let mut this = self.clone_lite();

        match method {
            "info" => {
                let info = TransportInfo {
                    id: TRANSPORT_ID.to_string(),
                    name: "Stalwart Archive".to_string(),
                    role: "archive".to_string(),
                    trust: if this.configured() {
                        "configured".to_string()
                    } else {
                        "unconfigured".to_string()
                    },
                    latency: 10,
                };
                Ok(serde_json::to_value(&info).unwrap_or_default())
            }
            "health" => {
                if !this.configured() {
                    let health = TransportHealth {
                        id: TRANSPORT_ID.to_string(),
                        name: "Stalwart Archive".to_string(),
                        role: "archive".to_string(),
                        connected: false,
                        latency_ms: 0,
                        last_error: Some("credentials not configured".to_string()),
                    };
                    return Ok(serde_json::to_value(&health).unwrap_or_default());
                }

                let start = std::time::Instant::now();
                match this.jmap_echo() {
                    Ok(true) => {
                        let latency = start.elapsed().as_millis() as u64;
                        let health = TransportHealth {
                            id: TRANSPORT_ID.to_string(),
                            name: "Stalwart Archive".to_string(),
                            role: "archive".to_string(),
                            connected: true,
                            latency_ms: latency,
                            last_error: None,
                        };
                        Ok(serde_json::to_value(&health).unwrap_or_default())
                    }
                    Ok(false) => {
                        let health = TransportHealth {
                            id: TRANSPORT_ID.to_string(),
                            name: "Stalwart Archive".to_string(),
                            role: "archive".to_string(),
                            connected: false,
                            latency_ms: 0,
                            last_error: Some("JMAP echo returned unexpected response".to_string()),
                        };
                        Ok(serde_json::to_value(&health).unwrap_or_default())
                    }
                    Err(e) => {
                        let health = TransportHealth {
                            id: TRANSPORT_ID.to_string(),
                            name: "Stalwart Archive".to_string(),
                            role: "archive".to_string(),
                            connected: false,
                            latency_ms: 0,
                            last_error: Some(e),
                        };
                        Ok(serde_json::to_value(&health).unwrap_or_default())
                    }
                }
            }
            "list" => match this.list_messages() {
                Ok(messages) => Ok(envelope_ok(
                    serde_json::to_value(&messages).unwrap_or_default(),
                )),
                Err(e) => {
                    if !this.configured() {
                        Ok(envelope_ok(serde_json::json!([])))
                    } else {
                        Ok(envelope_err(&e))
                    }
                }
            },
            "send" => {
                let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if text.is_empty() {
                    return Ok(envelope_ok(
                        serde_json::json!({"accepted": false, "reason": "empty"}),
                    ));
                }
                if !this.configured() {
                    return Ok(envelope_ok(serde_json::json!({
                        "accepted": false,
                        "reason": "stalwart not configured"
                    })));
                }
                match this.archive_message(text) {
                    Ok(resp) => Ok(envelope_ok(resp)),
                    Err(e) => Ok(envelope_err(&e)),
                }
            }
            _ => Err(format!("unknown stalwart method: {method}")),
        }
    }
}

impl StalwartClient {
    /// Create a clone with fresh session state (username/password preserved).
    /// The bridge RwLock ensures single access.
    fn clone_lite(&self) -> Self {
        Self {
            base_url: self.base_url.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
            api_url: None,
            account_id: None,
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn base64_encode(input: &str) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        output.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        output.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(n & 0x3F) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn utc_now() -> String {
    chrono_now().unwrap_or_else(|| "2024-01-01T00:00:00Z".to_string())
}

fn chrono_now() -> Option<String> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate date from days since UNIX epoch (simplified Gregorian)
    let mut y = 1970i64;
    let mut d = days_since_epoch as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        y += 1;
    }
    let month_days = days_per_month(y, d);
    let (month, day) = month_days;

    Some(format!(
        "{y:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z"
    ))
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_per_month(year: i64, day_of_year: i64) -> (i64, i64) {
    let feb = if is_leap(year) { 29 } else { 28 };
    let months = [31, feb, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut remaining = day_of_year;
    for (i, days) in months.iter().enumerate() {
        if remaining < *days {
            return ((i + 1) as i64, remaining + 1);
        }
        remaining -= *days;
    }
    (12, remaining + 1)
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalwart_unconfigured_health() {
        let client = StalwartClient {
            base_url: "http://localhost:9999".to_string(),
            username: None,
            password: None,
            api_url: None,
            account_id: None,
        };
        let result = client.invoke("health", &serde_json::json!({})).unwrap();
        let health: TransportHealth = serde_json::from_value(result).unwrap();
        assert!(!health.connected);
        assert!(health.last_error.is_some());
    }

    #[test]
    fn stalwart_info() {
        let client = StalwartClient::from_env();
        let result = client.invoke("info", &serde_json::json!({})).unwrap();
        let info: TransportInfo = serde_json::from_value(result).unwrap();
        assert_eq!(info.id, "stalwart");
        assert_eq!(info.role, "archive");
    }

    #[test]
    fn base64_encodes_correctly() {
        assert_eq!(base64_encode("admin:pass"), "YWRtaW46cGFzcw==");
        assert_eq!(base64_encode("test:secret"), "dGVzdDpzZWNyZXQ=");
    }

    #[test]
    fn truncate_works() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello…");
    }
}
