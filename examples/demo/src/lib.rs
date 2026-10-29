//! Demo: using eqswift to export a small Rust library to Swift.
//!
//! Build this example:
//! ```bash
//! cargo build --manifest-path examples/demo/Cargo.toml
//! ```
//!
//! Generate Swift bindings (from `examples/demo`):
//! ```bash
//! cargo eqswift swift --out-dir swift/Generated
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

/// A simple data record exported to Swift as a `struct`.
#[derive(eqswift::Record)]
pub struct Person {
    pub name: String,
    pub age: u32,
}

/// An object exported to Swift as a `class` with methods.
#[derive(eqswift::Object, Default)]
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
