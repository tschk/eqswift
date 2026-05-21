//! Shared transport types (vendored from Aurorality for Swift FFI).

/// Health snapshot returned by any transport.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransportHealth {
    pub id: String,
    pub name: String,
    pub role: String,
    pub connected: bool,
    pub latency_ms: u64,
    pub last_error: Option<String>,
}

/// Transport metadata for discovery / dashboard display.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransportInfo {
    pub id: String,
    pub name: String,
    pub role: String,
    pub trust: String,
    pub latency: u64,
}

/// Message envelope exchanged between transports.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransportMessage {
    pub id: String,
    pub text: String,
    pub transport: String,
    pub status: String,
    pub metadata: serde_json::Value,
}

/// Result of sending a message through a transport.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SendResult {
    pub accepted: bool,
    pub message_id: Option<String>,
    pub transport_message: Option<String>,
}

impl TransportHealth {
    pub fn unknown(id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: id.to_string(),
            role: "unknown".to_string(),
            connected: false,
            latency_ms: 0,
            last_error: Some("not configured".to_string()),
        }
    }
}

/// Helper: build a JSON envelope the plugin bridge expects.
pub fn envelope_ok(data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "ok": true, "data": data })
}

pub fn envelope_err(msg: &str) -> serde_json::Value {
    serde_json::json!({ "ok": false, "error": msg })
}
