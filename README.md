# eqswift

FFI framework only: UniFFI proc-macro wrappers + `cargo eqswift` for Swift bindings. No transports or app logic in the library crate — annotate Rust, build, generate Swift (no UDL, no `build.rs`).

## Quick Start

### 1. Add dependencies

```toml
[dependencies]
eqswift = "0.1"
uniffi = "0.31"
```

*(UniFFI must be a direct dependency because its proc-macros emit `::uniffi::...` paths.)*

### 2. Write Rust

```rust
// src/lib.rs
eqswift::setup!();

#[eqswift::export]
pub fn add(a: u32, b: u32) -> u32 {
    a + b
}

#[derive(eqswift::Record)]
pub struct Person {
    pub name: String,
    pub age: u32,
}

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
```

### 3. Build

```bash
cargo build
```

### 4. Generate Swift bindings

```bash
# Using cargo-eqswift (recommended)
cargo eqswift swift

# Or the long way with uniffi-bindgen directly
cargo run --bin uniffi-bindgen generate \
  --library target/debug/libeqswift.dylib \
  --language swift --out-dir swift/Generated
```

*(On Linux use `.so`, on Windows use `.dll`)*

### 5. Use from Swift

```swift
import eqswift

let sum = add(a: 1, b: 2)
let greeter = Greeter()
let msg = greeter.greet(name: "World")
let person = Person(name: "Alice", age: 30)
let msg2 = greeter.greetPerson(person: person)
```

## Project Layout

```
eqswift/
├── Cargo.toml                 # Workspace root
├── README.md                  # This file
├── AGENTS.md                  # Agent / contributor notes
├── eqswift-macros/            # Proc-macro crate
│   ├── Cargo.toml
│   └── src/lib.rs             # #[eqswift::export], eqswift::setup!()
├── eq-swift/                  # Your Rust library (crate name = "eqswift")
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs             # Your Rust code
│       └── bin/
│           └── uniffi-bindgen.rs   # Binding generator binary
├── cargo-eqswift/             # cargo subcommand
│   ├── Cargo.toml
│   └── src/main.rs            # cargo eqswift swift / build
├── examples/
│   └── otto-ffi/              # Real-world example: AI autocomplete backend
│       ├── Cargo.toml
│       └── src/lib.rs
└── swift/                     # Swift package
    ├── Package.swift
    └── Generated/             # cargo eqswift swift (not committed)
```

## Installing `cargo eqswift`

```bash
cargo install cargo-eqswift
```

Then use it from any eqswift project:

```bash
cargo eqswift swift                    # generate Swift bindings
cargo eqswift swift --release          # use release build
cargo eqswift build                    # cargo build + generate Swift
cargo eqswift swift --static           # static lib link hints (iOS ship)
```

## Detailed Usage

### `eqswift::setup!()`

Call **once** at the top of your `lib.rs`. It expands to `uniffi::setup_scaffolding!()` and configures the crate for proc-macro-based FFI.

```rust
eqswift::setup!();
```

### `#[eqswift::export]`

Annotate free functions or `impl` blocks to expose them to Swift.

**Free functions:**
```rust
#[eqswift::export]
pub fn calculate(x: f64) -> f64 {
    x * 2.0
}
```

**Object methods:**
```rust
#[derive(eqswift::Object)]
pub struct Calculator;

#[eqswift::export]
impl Calculator {
    #[uniffi::constructor]
    pub fn new() -> Self { Self }

    pub fn double(&self, x: f64) -> f64 {
        x * 2.0
    }
}
```

### Derive macros

| Macro | Rust type | Swift type |
|-------|-----------|------------|
| `#[derive(eqswift::Record)]` | `struct` with public fields | `struct` (value type) |
| `#[derive(eqswift::Object)]` | `struct` + `impl` block | `class` (reference type) |
| `#[derive(eqswift::Enum)]` | `enum` | `enum` |
| `#[derive(eqswift::Error)]` | `enum` implementing `Error` | `Error` |

### Supported types

Most Rust primitives and common types map directly:

| Rust | Swift |
|------|-------|
| `u32` | `UInt32` |
| `i32` | `Int32` |
| `u64` | `UInt64` |
| `f64` | `Double` |
| `bool` | `Bool` |
| `String` | `String` |
| `Vec<T>` | `[T]` |
| `Option<T>` | `T?` |
| `Result<T, E>` | throws |

See [UniFFI type docs](https://mozilla.github.io/uniffi-rs/latest/types/builtin_types.html) for the full list.

## Generating bindings

### With `cargo eqswift` (recommended)

```bash
# Generate Swift bindings (debug build)
cargo eqswift swift

# Generate Swift bindings (release build)
cargo eqswift swift --release

# Build + generate in one step
cargo eqswift build

# Static linking (uses libeqswift.a when built)
cargo eqswift swift --static --release
EQSWIFT_STATIC=1 EQSWIFT_PROFILE=release swift build
```

### With `uniffi-bindgen` directly

#### macOS

```bash
cargo run --bin uniffi-bindgen generate \
  --library target/debug/libeqswift.dylib \
  --language swift --out-dir eq-swift/swift/Generated
```

#### Linux

```bash
cargo run --bin uniffi-bindgen generate \
  --library target/debug/libeqswift.so \
  --language swift --out-dir eq-swift/swift/Generated
```

#### Windows

```bash
cargo run --bin uniffi-bindgen generate ^
  --library target/debug/eqswift.dll ^
  --language swift --out-dir eq-swift/swift/Generated
```

#### Release builds

Replace `target/debug/` with `target/release/` and add `--release` to `cargo build`.

#### Multi-platform / XCFramework

For shipping to iOS/macOS, wrap the generated Swift code and the Rust library in an XCFramework:

```bash
# Build for multiple targets
cargo build --release --target aarch64-apple-darwin
cargo build --release --target aarch64-apple-ios

# Generate bindings once (metadata is arch-agnostic)
cargo build -p eqswift --release --target aarch64-apple-darwin
cargo eqswift swift --release --target aarch64-apple-darwin --out-dir swift/Generated
```

## Swift Package integration

The generated files are:
- `eqswift.swift` — Swift types and API
- `eqswiftFFI.h` — C FFI header
- `eqswiftFFI.modulemap` — Clang module map

Run `cargo eqswift swift --out-dir swift/Generated` from the repo root. This repo's `swift/Package.swift` reads bindings from `swift/Generated/` and links `../../target/<profile>/libeqswift` (dynamic) or `-force_load` `libeqswift.a` when `EQSWIFT_STATIC=1`. Set `EQSWIFT_PROFILE=release` for release Rust artifacts.

## Troubleshooting

### "no bin target named `uniffi-bindgen`"

Make sure `eq-swift/Cargo.toml` has the `[[bin]]` section and `uniffi` is declared with `features = ["cli"]`.

### Generated Swift files are empty / old

Delete `eq-swift/swift/Generated/` and re-run the bindgen command.

### `dyld: Library not loaded`

When running Swift tests that load the Rust library, the dynamic linker needs to find the `.dylib`. Set:

```bash
export DYLD_LIBRARY_PATH="$(pwd)/target/debug:$DYLD_LIBRARY_PATH"
```

### Type not found in generated bindings

Only types used in `#[eqswift::export]` functions or marked with `#[derive(...)]` are exported. Make sure the type is public and appears in a signature or derive.

### `cargo eqswift` not found

Install it first:

```bash
cargo install cargo-eqswift
```

## How it works

1. `eqswift::setup!()` calls `uniffi::setup_scaffolding!()`, which sets up the proc-macro metadata system.
2. `#[eqswift::export]` wraps `#[uniffi::export]`, registering functions and methods in the metadata.
3. `#[derive(eqswift::Record)]` re-exports `uniffi::Record`, which implements the FFI conversion traits.
4. `cargo build` embeds all metadata into the compiled library.
5. `cargo eqswift` (or `uniffi-bindgen`) reads the metadata from the library and generates Swift bindings.

No UDL file is ever written or parsed. The entire interface is defined by your Rust code.

## Testing

Run the integration smoke tests:

```bash
cargo test -p eqswift --test integration
```

This verifies:
- Library compiles
- Swift bindings are generated
- Exported items appear in generated Swift

## License

MPL-2.0
