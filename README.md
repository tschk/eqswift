# eqswift

FFI framework only: UniFFI proc-macro wrappers + `cargo eqswift` for Swift bindings. No transports or app logic in the library crate — annotate Rust, build, generate Swift (no UDL, no `build.rs`, no `uniffi-bindgen` binary in your crate).

The `eqswift` library crate exports nothing over FFI itself: only your crate's items end up in the generated Swift and in your library's exported symbols.

**Platform status:** on macOS, the integration tests prove the whole path in a fresh crate: `cargo eqswift build` produces the library and bindings, and a SwiftPM executable links the Rust library (both `lib<crate>.dylib` via rpath and `lib<crate>.a`), calls a function, a constructor and method, and a throwing function, and prints the expected results. For iOS, `cargo eqswift xcframework` builds `lib<crate>.a` for `aarch64-apple-ios`, `aarch64-apple-ios-sim` + `x86_64-apple-ios` (lipo'd into one simulator slice) and `aarch64-apple-darwin` and packages them as an XCFramework; the tests check its three slices, build a SwiftPM package that consumes it as a `binaryTarget` for the iOS Simulator with `xcodebuild` (a dynamic library product, so the Rust archive is linked), and build and run it on macOS. Not tested: physical-device builds, running on a booted simulator, code signing, and Intel Macs (the macOS slice is arm64 only). CI also runs the Rust tests on Linux.

## Quick Start

### 1. Add the dependency

```toml
[dependencies]
eqswift = "0.1"
```

That is the only dependency: `eqswift::setup!()` aliases `eqswift` as `uniffi` in your crate root, so the `::uniffi::...` paths UniFFI's proc-macros emit (and `#[uniffi::constructor]`) resolve through `eqswift`. A crate that already depends on `uniffi = "0.32"` directly keeps working; `setup!()` sees it in `Cargo.toml` and skips the alias.

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

### 3. Build and generate Swift bindings

```bash
cargo eqswift build
```

Run it from your crate's directory. It builds your lib with `cargo rustc --lib --crate-type cdylib,staticlib`, so your `Cargo.toml` needs no `[lib] crate-type` (the default rlib is fine), producing `lib<crate>.dylib` (`.so` on Linux) and `lib<crate>.a`. It then reads the library and calls UniFFI's bindgen as a library, so your crate needs no `uniffi-bindgen` binary or extra setup. `cargo eqswift swift` only generates, from an existing build (`.a` preferred with `--static`), and runs the same build first if no library exists yet. For a package named `my-crate` it writes to `swift/Generated/`:

- `my_crate.swift` — Swift API
- `my_crateFFI.h` — C FFI header
- `my_crateFFI.modulemap` and `module.modulemap` (same content; the latter is the name SwiftPM `systemLibrary` targets expect)

Use `--module-name MyKit` to name the Swift module `MyKit` instead (`MyKit.swift`, `MyKitFFI.h`, `module MyKitFFI`).

### 4. Use from Swift

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
cargo eqswift swift                    # generate Swift bindings (builds first if no library exists)
cargo eqswift swift --release          # use release build
cargo eqswift build                    # build cdylib + staticlib, then generate Swift
cargo eqswift swift --static           # prefer lib<crate>.a, print static link hints
cargo eqswift swift --module-name MyKit  # override the Swift module name
cargo eqswift xcframework              # iOS + iOS Simulator + macOS XCFramework (arm64)
```

## Detailed Usage

### `eqswift::setup!()`

Call **once** at the top of your `lib.rs` (the crate root). It expands to `uniffi::setup_scaffolding!()` and configures the crate for proc-macro-based FFI. Unless your `Cargo.toml` lists `uniffi` under `[dependencies]`, it also adds `extern crate eqswift as uniffi;`, which is why no direct `uniffi` dependency is needed.

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

# Build (cdylib + staticlib, no crate-type setup needed) + generate in one step
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

### iOS / XCFramework

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin
cargo eqswift xcframework [--release] [--out-dir swift/XCFramework] [--module-name MyKit]
```

This needs Xcode. It builds `lib<crate>.a` for `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios` and `aarch64-apple-darwin` (`cargo rustc --crate-type staticlib --target …`), lipos the two simulator archives together (Xcode's generic iOS Simulator destination builds arm64 and x86_64, so an arm64-only slice fails to link), generates the Swift bindings once, and runs `xcodebuild -create-xcframework` with one `-library`/`-headers` pair per slice. The headers are `<module>FFI.h` + `module.modulemap` in `Headers/<module>FFI/`. For a crate `my_crate` the output directory contains:

```
swift/XCFramework/
├── my_crateFFI.xcframework   # ios-arm64, ios-arm64_x86_64-simulator, macos-arm64
└── my_crate.swift
```

Use it from SwiftPM (the command prints these lines):

```swift
.binaryTarget(name: "my_crateFFI", path: "Frameworks/my_crateFFI.xcframework"),
.target(name: "my_crate", dependencies: ["my_crateFFI"], path: "Sources/my_crate"),  // my_crate.swift
```

In an Xcode project, add the `.xcframework` to the target and `my_crate.swift` to its sources. The `xcframework_builds_for_ios_simulator_and_macos` integration test builds such a package for the iOS Simulator (`xcodebuild -destination 'generic/platform=iOS Simulator'`) and for macOS (`swift build`). The macOS slice is arm64 only; device builds, simulator runs and code signing are not tested.

For one other target, `cargo eqswift build --target <triple>` builds it and `cargo eqswift swift --target <triple>` reads from `target/<triple>/<profile>/`.

## Swift Package integration

Point a SwiftPM `systemLibrary` target named `<module>FFI` at the output directory (it uses `module.modulemap`), add `<module>.swift` to a Swift target that depends on it, and link the Rust library with the flags `cargo eqswift` prints. On macOS this is what the `swift_package_links_and_calls_rust` integration test does:

```swift
.systemLibrary(name: "my_crateFFI", path: "Sources/my_crateFFI"),  // my_crateFFI.h + module.modulemap
.target(
    name: "my_crate",                                                // my_crate.swift
    dependencies: ["my_crateFFI"],
    linkerSettings: [.unsafeFlags([
        "-L", "<target>/debug", "-lmy_crate",                       // dylib
        "-Xlinker", "-rpath", "-Xlinker", "<target>/debug",
        // or static: "<target>/debug/libmy_crate.a"
    ])]
),
```

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

Make sure your crate calls `eqswift::setup!()`. `cargo eqswift swift` reuses an existing library, so after changing exports run `cargo eqswift build` (a plain `cargo build` only refreshes the library if your `Cargo.toml` sets a `cdylib`/`staticlib` crate-type). If your `uniffi.toml` sets `[bindings.swift] module_name`, pass the same value with `--module-name`.

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

1. `eqswift::setup!()` calls `uniffi::setup_scaffolding!()`, which sets up the proc-macro metadata system, and (without a direct `uniffi` dependency) adds `extern crate eqswift as uniffi;` — `eqswift` re-exports UniFFI's whole API, so UniFFI's generated `::uniffi::...` paths resolve through it.
2. `#[eqswift::export]` wraps `#[uniffi::export]`, registering functions and methods in the metadata.
3. `#[derive(eqswift::Record)]` re-exports `uniffi::Record`, which implements the FFI conversion traits.
4. `cargo eqswift build` (`cargo rustc --lib --crate-type cdylib,staticlib`) embeds all metadata into the compiled library.
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
- A fresh crate whose only dependency is `eqswift` exports functions, records, errors, and objects with constructors
- On macOS (skipped without `swift`): a SwiftPM executable links a fresh crate's dylib and, separately, its static `.a`, calls exported functions, a constructor, a method, and a throwing function, and prints the expected output
- A fresh crate with no `[lib] crate-type` gets `lib<crate>.dylib`, `lib<crate>.a`, and bindings from `cargo eqswift build` (and from `cargo eqswift swift` when nothing is built yet)
- A user crate's library contains no UniFFI symbols from `eqswift` itself

## License

ISC
