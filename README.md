# eqswift

FFI framework only: UniFFI proc-macro wrappers + `cargo eqswift` for Swift bindings. No transports or app logic in the library crate — annotate Rust, build, generate Swift (no UDL, no `build.rs`, no `uniffi-bindgen` binary in your crate).

The `eqswift` library crate exports nothing over FFI itself: only your crate's items end up in the generated Swift and in your library's exported symbols.

**Platform status:** binding generation and the integration tests are verified on macOS hosts (CI also runs the Rust tests on Linux). `cargo eqswift` does not build iOS libraries or XCFrameworks for you; `--target`/`--static` only locate an already-built library and print linker hints, and iOS linking is not covered by tests.

## Quick Start

### 1. Add dependencies

```toml
[dependencies]
eqswift = "0.1"
uniffi = "0.32"
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
cargo eqswift swift
```

Run it from your crate's directory. It reads the built `lib<crate>.dylib` (`.so` on Linux, `.a` with `--static`) and calls UniFFI's bindgen as a library, so your crate needs no `uniffi-bindgen` binary or extra setup. For a package named `my-crate` it writes to `swift/Generated/`:

- `my_crate.swift` — Swift API
- `my_crateFFI.h` — C FFI header
- `my_crateFFI.modulemap` and `module.modulemap` (same content; the latter is the name SwiftPM `systemLibrary` targets expect)

Use `--module-name MyKit` to name the Swift module `MyKit` instead (`MyKit.swift`, `MyKitFFI.h`, `module MyKitFFI`).

### 5. Use from Swift

```swift
import my_crate  // or whatever your Swift target is called

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
├── eq-swift/                  # Library crate users depend on (crate name = "eqswift")
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs             # Re-exports only; exports nothing over FFI
│   │   └── bin/
│   │       └── uniffi-bindgen.rs   # Plain UniFFI CLI (`--features cli`), for this repo
│   └── tests/integration.rs   # Demo + external-crate end-to-end tests
├── cargo-eqswift/             # cargo subcommand
│   ├── Cargo.toml
│   └── src/main.rs            # cargo eqswift swift / build
├── examples/
│   ├── demo/                  # Demo exports (Person, Greeter, add, Task, …)
│   └── otto-ffi/              # Real-world example: AI autocomplete backend
│       ├── Cargo.toml
│       └── src/lib.rs
└── swift/                     # Sample Swift package for examples/demo
    ├── Package.swift
    └── Generated/             # cargo eqswift swift (not committed)
```

## Installing `cargo eqswift`

```bash
cargo install cargo-eqswift            # or: cargo install --path cargo-eqswift
```

Then use it from any eqswift project:

```bash
cargo eqswift swift                    # generate Swift bindings
cargo eqswift swift --release          # use release build
cargo eqswift build                    # cargo build + generate Swift
cargo eqswift swift --static           # prefer lib<crate>.a, print static link hints
cargo eqswift swift --module-name MyKit  # override the Swift module name
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

# Static linking (uses lib<crate>.a when built)
cargo eqswift swift --static --release
```

### With `uniffi-bindgen` directly

`cargo eqswift` is a thin wrapper over UniFFI's library-mode bindgen. To call it yourself, follow UniFFI's docs to add a `uniffi-bindgen` binary to your project, then:

```bash
uniffi-bindgen generate --library target/debug/libmy_crate.dylib \
  --language swift --out-dir swift/Generated
```

### Multi-platform / XCFramework

Not automated. `cargo eqswift` only generates bindings from a library you have already built, for one target at a time (`--target <triple>` looks in `target/<triple>/<profile>/`). Building for iOS targets and packaging an XCFramework (e.g. `xcodebuild -create-xcframework`) is up to you and is not tested in this repo.

## Swift Package integration

Point a SwiftPM `systemLibrary` target named `<module>FFI` at the output directory (it uses `module.modulemap`), add `<module>.swift` to a Swift target that depends on it, and link the Rust library with the flags `cargo eqswift` prints.

This repo's `swift/Package.swift` is a sample for `examples/demo`:

```bash
cd examples/demo
CARGO_TARGET_DIR=../../target cargo build
CARGO_TARGET_DIR=../../target cargo eqswift swift --out-dir ../../swift/Generated
cd ../../swift && swift build
```

It links `target/<profile>/libeqswift_demo` (dynamic) or `-force_load` `libeqswift_demo.a` when `EQSWIFT_STATIC=1`. Set `EQSWIFT_PROFILE=release` for release Rust artifacts.

## Troubleshooting

### `expected …FFI.modulemap to be generated`

Make sure your crate calls `eqswift::setup!()` and was built (`cargo build`) before `cargo eqswift swift`. If your `uniffi.toml` sets `[bindings.swift] module_name`, pass the same value with `--module-name`.

### Generated Swift files are empty / old

Delete the output directory (default `swift/Generated/`) and re-run `cargo eqswift swift`.

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
5. `cargo eqswift` calls UniFFI's bindgen (linked in as a library) to read the metadata from the built library and generate Swift bindings named after your crate.

No UDL file is ever written or parsed. The entire interface is defined by your Rust code.

## Testing

Run the integration tests:

```bash
cargo test -p eqswift --test integration
```

This verifies:
- `examples/demo` builds and its exported items appear in generated Swift
- `cargo eqswift swift` works in a fresh crate outside this workspace, names the output after that crate, and honours `--module-name`
- A user crate's library contains no UniFFI symbols from `eqswift` itself

## License

ISC
