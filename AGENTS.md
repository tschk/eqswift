# Agent Notes — eqswift

This file contains build steps, conventions, and context for AI agents working on this project.

## Project Structure

This is a Cargo workspace with three crates:

- **`eqswift-macros/`** — proc-macro crate (`proc-macro = true`)
  - `#[eqswift::export]` — wraps `#[uniffi::export]`
  - `eqswift::setup!()` — wraps `uniffi::setup_scaffolding!()`; also emits `extern crate eqswift as uniffi;` gated off where the user's `Cargo.toml` gives the crate a real `uniffi`: none for `[dependencies]`, `#[cfg(not(p))]` for `[target.'cfg(p)'.dependencies]`, `#[cfg(not(test))]` for `[dev-dependencies]`
- **`eq-swift/`** — the main library crate (library name = `eqswift`)
  - `src/lib.rs` — re-exports only (including `pub use uniffi::*`); it must not call `setup!()` or export anything (users would get those symbols/bindings)
  - `src/bin/uniffi-bindgen.rs` — plain UniFFI CLI (`--features cli`), used by this repo's tests
  - `tests/integration.rs` — demo bindings + external-crate end-to-end tests
- **`cargo-eqswift/`** — cargo subcommand for better DX
  - `cargo eqswift swift` — generate Swift bindings (calls `uniffi_bindgen` as a library; works in any crate)
  - `cargo eqswift build` — `cargo rustc --lib --crate-type cdylib,staticlib` + generate Swift (no crate-type setup needed in the user crate)
  - `cargo eqswift swift` runs that same build first when no library artifact exists
  - `cargo eqswift swift --static` — prefer `lib<crate>.a` and print static link hints (macOS static link tested; iOS untested)
  - `cargo eqswift swift --module-name Foo` — override the Swift module name
- **`examples/demo/`** — demo exports (`Person`, `Greeter`, `add`, `version`, `Task`, …); separate workspace

## Build Steps

1. **Build the Rust library:**
   ```bash
   cargo build
   # or for release:
   cargo build --release
   ```

2. **Generate Swift bindings (via cargo-eqswift):**
   ```bash
   cargo eqswift swift
   # or with explicit output directory:
   cargo eqswift swift --out-dir swift/Generated
   ```

3. **Or, in this repo, generate the demo via uniffi-bindgen directly:**
   ```bash
   CARGO_TARGET_DIR=target cargo build --manifest-path examples/demo/Cargo.toml
   cargo run -p eqswift --features cli --bin uniffi-bindgen -- generate \
     --library target/debug/libeqswift_demo.dylib \
     --language swift --out-dir /tmp/eqswift-demo
   ```

4. **Verify bindings were created** (names follow the crate's lib name):
   ```bash
   ls swift/Generated/
   # for crate my_crate: my_crate.swift, my_crateFFI.h, my_crateFFI.modulemap, module.modulemap
   ```

5. **Run integration tests:**
   ```bash
   cargo test -p eqswift --test integration
   ```

## Key Conventions

- **No UDL files.** The project uses UniFFI's proc-macro mode exclusively.
- **No build.rs.** Everything is driven by proc macros at compile time.
- **Library crate-type:** user crates need none; `cargo eqswift` builds `cdylib`/`staticlib` via `cargo rustc --crate-type`. `examples/*` declare them so plain `cargo build` works for this repo's tests; the `eqswift` crate itself is a plain rlib.
- **Proc-macro re-exports:** `eqswift::Record`, `eqswift::Object`, `eqswift::Enum`, `eqswift::Error` are just re-exports of `uniffi::Record`, `uniffi::Object`, etc. This lets users write `#[derive(eqswift::Record)]` instead of `#[derive(uniffi::Record)]`.
- **Binary name:** `uniffi-bindgen` (repo-only helper, `--features cli`). `cargo eqswift` does not need it.
- **cargo subcommand:** `cargo-eqswift`. Install with `cargo install --path cargo-eqswift`.

## UniFFI is not a required direct dependency

Downstream crates need only `eqswift = "0.1"`. UniFFI's proc-macros emit `::uniffi::...` paths (and users write `#[uniffi::constructor]`, which UniFFI matches by its first path segment). `setup!()` emits `extern crate eqswift as uniffi;` at the crate root; root `extern crate` items join the extern prelude, so those paths resolve to `eqswift`, which glob re-exports `uniffi::*`.

If the user's crate also depends on `uniffi` directly, the alias would be ambiguous with the `--extern uniffi` crate (E0659), so `setup!()` reads `$CARGO_MANIFEST_DIR/Cargo.toml` and gates the alias off wherever a real `uniffi` is present (see above). `examples/*` keep their direct `uniffi = "0.32"` to cover the no-alias path; `inactive_target_and_dev_uniffi_keep_the_alias_working` covers the cfg-gated paths. Remaining gap: a target table keyed by a triple (`[target.x86_64-apple-darwin.dependencies]`) can't be expressed as a `cfg`, so it is treated as always active.

## Adding New Macros

If you need to add a new derive or attribute macro to `eqswift-macros`:

1. Add the proc-macro definition in `eqswift-macros/src/lib.rs`.
2. Re-export it in `eq-swift/src/lib.rs` via `pub use eqswift_macros::your_macro;`.
3. Update the rustdoc example in `eq-swift/src/lib.rs` and `README.md`.

## Modifying the Rust API

When editing `eq-swift/src/lib.rs`:

- Do not add exported items or `setup!()` here; put demo/smoke-test exports in `examples/demo/src/lib.rs`.
- Remember that only `pub` items in `#[eqswift::export]` or with `#[derive(...)]` are visible to Swift.
- If you add a new exported function, rebuild and regenerate bindings to verify.

## Swift Side

The `swift/` directory is an optional sample Swift package. Generated bindings go in `swift/Generated/` via `cargo eqswift swift --out-dir swift/Generated` (gitignored). This repo is an FFI framework only — no app transports or product logic in the `eqswift` crate.

- `swift/Package.swift` is a sample for `examples/demo`: it consumes `Generated/` and links `target/<profile>/libeqswift_demo` (see `EQSWIFT_PROFILE`, `EQSWIFT_STATIC`).

## Testing

Integration tests live in `eq-swift/tests/integration.rs` and verify:

1. `examples/demo` builds and its exports (`Person`, `Greeter`, `add`, `greet`) appear in generated Swift
2. `cargo eqswift swift` works in a temp crate outside the workspace and names output after it (plus `--module-name`); that crate depends only on `eqswift` and exports functions, a record, an error, and an object with a constructor
3. A temp crate with no `[lib] crate-type` gets `.dylib`/`.a` + bindings from `cargo eqswift build` and `cargo eqswift swift`
4. A user crate's dylib contains no `eqswift` UniFFI symbols
5. macOS only (`swift_package_links_and_calls_rust`, skipped if `swift` is missing): a temp SwiftPM package (systemLibrary FFI target + generated Swift target + executable) links the temp crate's dylib (rpath) and then its `.a`, runs, and its printed output is asserted. SwiftPM's `.build` lives in a temp dir. CI runs it in the macOS job. iOS/XCFramework is out of scope.

Nested builds use `CARGO_TARGET_DIR=<repo>/target` and `-j2`.

## Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| `no bin target named uniffi-bindgen` | Running the repo-only helper without `--features cli` | `cargo run -p eqswift --features cli --bin uniffi-bindgen` |
| `setup_scaffolding!` not found | Wrong UniFFI version or missing `setup!()` | Ensure `eqswift::setup!()` is called |
| Empty / stale generated Swift | `cargo eqswift swift` reused an old library | Run `cargo eqswift build` |
| Type not in Swift | Not public / not in exported signature | Make type `pub` and use it in an exported function |
| `cargo eqswift` not found | Not installed | Run `cargo install --path cargo-eqswift` |
| `expected …FFI.modulemap to be generated` | Crate not built, no `setup!()`, or `uniffi.toml` renames the module | Build first; pass matching `--module-name` |

## Dependencies

- `uniffi = "0.32"` — core FFI framework (re-exported by `eqswift`; users don't add it)
- `toml` (in `eqswift-macros`) — `setup!()` reads the user manifest to detect a direct `uniffi` dependency
- `eqswift-macros` — our thin proc-macro wrappers
- `cargo-eqswift` — cargo subcommand for binding generation (depends on `uniffi_bindgen = "0.32"`)

Do not bump the UniFFI minor version without checking the [upgrade guide](https://mozilla.github.io/uniffi-rs/latest/Upgrading.html).
