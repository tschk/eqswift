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
//! cargo eqswift swift --out-dir swift/Generated
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

pub use uniffi::Enum;
pub use uniffi::Error;
pub use uniffi::Object;
pub use uniffi::Record;

#[doc(hidden)]
pub use uniffi::constructor as __uniffi_constructor;
#[doc(hidden)]
pub use uniffi::export as __uniffi_export;
#[doc(hidden)]
pub use uniffi::setup_scaffolding;

extern crate self as eqswift;

eqswift::setup!();

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
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self
    }

    pub fn greet(&self, name: String) -> String {
        format!("Hello, {name}!")
    }

    pub fn greet_person(&self, person: Person) -> String {
        format!("Hello, {}! You are {} years old.", person.name, person.age)
    }
}

#[eqswift::export]
pub fn add(a: u32, b: u32) -> u32 {
    a + b
}

#[eqswift::export]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}