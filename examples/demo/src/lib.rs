//! Demo: using eqswift to export a small Rust library to Swift.
//!
//! Build this example:
//! ```bash
//! cargo build --manifest-path examples/demo/Cargo.toml
//! ```
//!
//! Generate Swift bindings:
//! ```bash
//! cargo run --bin uniffi-bindgen generate \
//!   --library target/debug/libeqswift_demo.dylib \
//!   --language swift --out-dir examples/demo/swift
//! ```

eqswift::setup!();

use eqswift::Record;

#[derive(Record)]
pub struct Task {
    pub id: u32,
    pub title: String,
    pub done: bool,
}

#[derive(eqswift::Object)]
pub struct TaskManager;

#[eqswift::export]
impl TaskManager {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self
    }

    pub fn create_task(&self, title: String) -> Task {
        Task {
            id: 1,
            title,
            done: false,
        }
    }

    pub fn toggle(&self, mut task: Task) -> Task {
        task.done = !task.done;
        task
    }
}

#[eqswift::export]
pub fn greet(name: String) -> String {
    format!("Hello, {name}!")
}
