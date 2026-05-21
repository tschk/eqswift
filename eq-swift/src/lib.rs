//! eq-swift — Zero-config Rust-to-Swift FFI
//!
//! A thin, ergonomic wrapper around [UniFFI](https://github.com/mozilla/uniffi-rs)
//! that lets you write your entire foreign interface in Rust — no UDL files, no
//! `build.rs`, no duplication.
//!
//! Just annotate your items with `#[eqswift::export]` or `#[derive(eqswift::Record)]`,
//! call `eqswift::setup!()` once at the top of `lib.rs`, and build.
//!
//! # Quick Start
//!
//! ```ignore
//! eqswift::setup!();
//!
//! #[eqswift::export]
//! pub fn add(a: u32, b: u32) -> u32 {
//!     a + b
//! }
//!
//! #[derive(eqswift::Record)]
//! pub struct Person {
//!     pub name: String,
//!     pub age: u32,
//! }
//!
//! #[derive(eqswift::Object)]
//! pub struct Greeter;
//!
//! #[eqswift::export]
//! impl Greeter {
//!     // Automatically detected as constructor — no attribute needed
//!     pub fn new() -> Self {
//!         Self
//!     }
//!
//!     pub fn greet(&self, name: String) -> String {
//!         format!("Hello, {name}!")
//!     }
//! }
//! ```
//!
//! Then build and generate Swift bindings:
//! ```bash
//! cargo build
//! cargo eqswift swift --out-dir eq-swift/swift/Generated
//! ```
//!
//! # Macros
//!
//! | Macro | Purpose |
//! |-------|---------|
//! | [`setup!`](eqswift_macros::setup) | One-time initialization. Call once at the top of `lib.rs`. |
//! | [`export`](eqswift_macros::export) | Mark a free function or `impl` block for export. Auto-detects constructors. |
//! | [`Record`](uniffi::Record) | Derive for plain data structs (Swift `struct`). |
//! | [`Object`](uniffi::Object) | Derive for reference types with methods (Swift `class`). |
//! | [`Enum`](uniffi::Enum) | Derive for enums. |
//! | [`Error`](uniffi::Error) | Derive for error enums. |
//!
//! # Supported Types
//!
//! Most Rust primitives map directly to Swift:
//!
//! | Rust | Swift |
//! |------|-------|
//! | `u32` | `UInt32` |
//! | `i32` | `Int32` |
//! | `f64` | `Double` |
//! | `bool` | `Bool` |
//! | `String` | `String` |
//! | `Vec<T>` | `[T]` |
//! | `Option<T>` | `T?` |
//! | `Result<T, E>` | `throws` |
//!
//! See the [UniFFI type docs](https://mozilla.github.io/uniffi-rs/latest/types/builtin_types.html)
//! for the complete list.

pub use eqswift_macros::{export, setup};

// Re-export UniFFI derives so users can write `#[derive(eqswift::Record)]` etc.
pub use uniffi::Enum;
pub use uniffi::Error;
pub use uniffi::Object;
pub use uniffi::Record;

// Re-export UniFFI internals so eqswift-macros can emit paths that work
// in downstream crates without requiring uniffi as a direct dependency.
#[doc(hidden)]
pub use uniffi::constructor as __uniffi_constructor;
#[doc(hidden)]
pub use uniffi::export as __uniffi_export;
#[doc(hidden)]
pub use uniffi::setup_scaffolding;

// Allow using `eqswift::` paths inside this crate too.
extern crate self as eqswift;

// ---------------------------------------------------------------------------
// Transports + demo API (exported to Swift via UniFFI)
// ---------------------------------------------------------------------------

eqswift::setup!();

mod transport_types;
mod matrix;
mod stalwart;

fn json_result(result: Result<serde_json::Value, String>) -> String {
    match result {
        Ok(v) => serde_json::to_string(&v).unwrap_or_else(|_| "{}".to_string()),
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

/// Matrix Client-Server health snapshot as JSON (uses `MATRIX_*` env).
#[eqswift::export]
pub fn matrix_health_json() -> String {
    let c = matrix::MatrixClient::from_env();
    json_result(c.invoke("health", &serde_json::json!({})))
}

/// Send plain text to the configured Matrix room (`MATRIX_ROOM_ID`).
#[eqswift::export]
pub fn matrix_send_json(text: String) -> String {
    let c = matrix::MatrixClient::from_env();
    json_result(c.invoke("send", &serde_json::json!({ "text": text })))
}

/// Stalwart JMAP health snapshot (`STALWART_*` env).
#[eqswift::export]
pub fn stalwart_health_json() -> String {
    let c = stalwart::StalwartClient::from_env();
    json_result(c.invoke("health", &serde_json::json!({})))
}

/// Archive a short text payload via Stalwart JMAP (`STALWART_*` env).
#[eqswift::export]
pub fn stalwart_send_json(text: String) -> String {
    let c = stalwart::StalwartClient::from_env();
    json_result(c.invoke("send", &serde_json::json!({ "text": text })))
}

/// Bitchat: upstream SwiftPM is executable-only — static status JSON for UI.
#[eqswift::export]
pub fn bitchat_status_json() -> String {
    serde_json::json!({
        "id": "bitchat",
        "name": "Bitchat",
        "role": "mesh",
        "connected": false,
        "latency_ms": 0,
        "last_error": "permissionlesstech/bitchat exposes an executable product only — no Swift library to link yet."
    })
    .to_string()
}

/// A simple data record exported to Swift as a `struct`.
#[derive(eqswift::Record)]
pub struct Person {
    pub name: String,
    pub age: u32,
}

/// An object exported to Swift as a `class` with methods.
#[derive(eqswift::Object)]
pub struct Greeter;

#[eqswift::export]
impl Greeter {
    /// Default constructor.
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self
    }

    /// Greet someone by name.
    pub fn greet(&self, name: String) -> String {
        format!("Hello, {name}!")
    }

    /// Greet a [`Person`].
    pub fn greet_person(&self, person: Person) -> String {
        format!("Hello, {}! You are {} years old.", person.name, person.age)
    }
}

/// Add two numbers.
#[eqswift::export]
pub fn add(a: u32, b: u32) -> u32 {
    a + b
}

/// Return the crate version string.
#[eqswift::export]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
