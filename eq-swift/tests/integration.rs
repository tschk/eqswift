//! Integration smoke test — build the demo crate, generate Swift bindings, verify output.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn target_dir() -> PathBuf {
    workspace_root().join("target").join("debug")
}

fn library_path() -> PathBuf {
    target_dir().join(format!(
        "{}eqswift_demo{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ))
}

/// The `eqswift` library exports nothing itself; the demo crate (`examples/demo`) is the
/// cdylib UniFFI bindgen reads.
fn ensure_cdylib() {
    let status = Command::new("cargo")
        .args(["build", "-j2", "--quiet", "--manifest-path"])
        .arg(workspace_root().join("examples/demo/Cargo.toml"))
        .env("CARGO_TARGET_DIR", workspace_root().join("target"))
        .status()
        .expect("cargo build examples/demo should run");
    assert!(status.success(), "cargo build examples/demo should succeed");
    let dylib = library_path();
    assert!(dylib.exists(), "library should exist: {}", dylib.display());
}

fn generate_swift(out_dir: &std::path::Path) -> std::process::ExitStatus {
    Command::new("cargo")
        .args([
            "run",
            "-p",
            "eqswift",
            "--features",
            "cli",
            "-j2",
            "--quiet",
            "--bin",
            "uniffi-bindgen",
            "--",
            "generate",
            "--library",
        ])
        .arg(library_path())
        .args(["--language", "swift", "--out-dir"])
        .arg(out_dir)
        .status()
        .expect("uniffi-bindgen should run")
}

/// Create a throwaway crate outside this workspace that depends on `eqswift` by path,
/// the way a user would, and build it with a `cdylib` crate-type. Builds share this repo's
/// `target/` dir.
fn external_crate(package_name: &str) -> tempfile::TempDir {
    let dir = write_external_crate(package_name, "[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n");
    let status = Command::new("cargo")
        .args(["build", "-j2", "--quiet"])
        .current_dir(dir.path())
        .env("CARGO_TARGET_DIR", workspace_root().join("target"))
        .status()
        .expect("cargo build should run");
    assert!(status.success(), "external crate should build");
    dir
}

/// Write (but don't build) the throwaway crate; `lib_section` is inserted verbatim.
fn write_external_crate(package_name: &str, lib_section: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let eqswift_path = workspace_root().join("eq-swift");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!(
            r#"[package]
name = "{package_name}"
version = "0.1.0"
edition = "2021"
publish = false

[workspace]

{lib_section}
[dependencies]
eqswift = {{ path = {eqswift_path:?} }}
"#
        ),
    )
    .unwrap();
    // Reuse this workspace's resolved versions so the shared target dir is reused.
    std::fs::copy(
        workspace_root().join("Cargo.lock"),
        dir.path().join("Cargo.lock"),
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/lib.rs"),
        r#"eqswift::setup!();

#[eqswift::export]
pub fn shout(s: String) -> String {
    s.to_uppercase()
}

#[derive(eqswift::Record)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, eqswift::Error)]
pub enum MathError {
    DivideByZero,
}

impl std::fmt::Display for MathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "divide by zero")
    }
}

#[eqswift::export]
pub fn divide(a: i32, b: i32) -> Result<i32, MathError> {
    if b == 0 { Err(MathError::DivideByZero) } else { Ok(a / b) }
}

#[derive(eqswift::Object)]
pub struct Counter {
    start: Point,
}

#[eqswift::export]
impl Counter {
    #[uniffi::constructor]
    pub fn new(start: Point) -> Self {
        Self { start }
    }

    pub fn sum(&self) -> i32 {
        self.start.x + self.start.y
    }
}
"#,
    )
    .unwrap();
    dir
}

/// Run `cargo eqswift <subcommand>` (from this workspace's sources) inside `crate_dir`.
fn run_cargo_eqswift_cmd(
    subcommand: &str,
    crate_dir: &std::path::Path,
    out_dir: &std::path::Path,
    extra_args: &[&str],
) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "-j2", "--quiet", "--manifest-path"])
        .arg(workspace_root().join("Cargo.toml"))
        .args([
            "-p",
            "cargo-eqswift",
            "--",
            "eqswift",
            subcommand,
            "--out-dir",
        ])
        .arg(out_dir)
        .args(extra_args)
        .current_dir(crate_dir)
        .env("CARGO_TARGET_DIR", workspace_root().join("target"))
        .env("CARGO_BUILD_JOBS", "2")
        .output()
        .expect("cargo eqswift should run")
}

fn run_cargo_eqswift(
    crate_dir: &std::path::Path,
    out_dir: &std::path::Path,
    extra_args: &[&str],
) -> std::process::Output {
    run_cargo_eqswift_cmd("swift", crate_dir, out_dir, extra_args)
}

fn cargo_eqswift_swift(crate_dir: &std::path::Path, extra_args: &[&str]) -> tempfile::TempDir {
    let out_dir = tempfile::tempdir().unwrap();
    let output = run_cargo_eqswift(crate_dir, out_dir.path(), extra_args);
    assert!(
        output.status.success(),
        "cargo eqswift swift should succeed in an external crate:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    out_dir
}

#[test]
fn external_crate_swift_bindings_named_after_crate() {
    let krate = external_crate("acme-widgets");
    let out_dir = cargo_eqswift_swift(krate.path(), &[]);

    let swift = out_dir.path().join("acme_widgets.swift");
    let header = out_dir.path().join("acme_widgetsFFI.h");
    let modulemap = out_dir.path().join("module.modulemap");
    assert!(swift.exists(), "acme_widgets.swift should be generated");
    assert!(header.exists(), "acme_widgetsFFI.h should be generated");
    assert!(modulemap.exists(), "module.modulemap should be generated");
    assert!(
        std::fs::read_to_string(&modulemap)
            .unwrap()
            .contains("module acme_widgetsFFI"),
        "module.modulemap should declare acme_widgetsFFI"
    );
    assert!(
        std::fs::read_to_string(&swift)
            .unwrap()
            .contains("public func shout"),
        "shout function missing"
    );
    assert!(
        !out_dir.path().join("eqswift.swift").exists(),
        "no eqswift.swift should be generated for a user crate"
    );

    let renamed = cargo_eqswift_swift(krate.path(), &["--module-name", "AcmeKit"]);
    assert!(
        renamed.path().join("AcmeKit.swift").exists(),
        "--module-name should rename the Swift file"
    );
    assert!(
        std::fs::read_to_string(renamed.path().join("module.modulemap"))
            .unwrap()
            .contains("module AcmeKitFFI"),
        "--module-name should rename the FFI module"
    );

    // UniFFI joins the module name onto the output dir; a path must be refused, not written.
    let parent = tempfile::tempdir().unwrap();
    let nested = parent.path().join("out");
    std::fs::create_dir(&nested).unwrap();
    let escaped = run_cargo_eqswift(krate.path(), &nested, &["--module-name", "../Escaped"]);
    assert!(
        !escaped.status.success(),
        "a path-like --module-name must be rejected"
    );
    assert!(
        !parent.path().join("Escaped.swift").exists(),
        "--module-name must not write outside --out-dir"
    );

    // A crate's uniffi.toml may rename the FFI modulemap file; module.modulemap must follow it.
    std::fs::write(
        krate.path().join("uniffi.toml"),
        "[bindings.swift]\nffi_module_filename = \"acme_map\"\n",
    )
    .unwrap();
    let configured = cargo_eqswift_swift(krate.path(), &[]);
    assert!(configured.path().join("acme_map.modulemap").exists());
    assert!(
        std::fs::read_to_string(configured.path().join("module.modulemap"))
            .unwrap()
            .contains("module acme_widgetsFFI"),
        "module.modulemap should come from the configured ffi_module_filename"
    );
}

/// A crate whose only dependency is `eqswift` (no direct `uniffi`) exports every item kind.
#[test]
fn external_crate_needs_only_eqswift_dependency() {
    let krate = external_crate("acme-solo");
    let manifest = std::fs::read_to_string(krate.path().join("Cargo.toml")).unwrap();
    assert!(
        !manifest.contains("uniffi"),
        "temp crate must not depend on uniffi"
    );
    let out_dir = cargo_eqswift_swift(krate.path(), &[]);
    let swift = std::fs::read_to_string(out_dir.path().join("acme_solo.swift")).unwrap();
    // UniFFI's templates space signatures irregularly; compare with whitespace removed.
    let compact: String = swift.split_whitespace().collect();
    for needle in [
        "public func shout(s: String) -> String",
        "public struct Point",
        "enum MathError: Swift.Error",
        "public func divide(a: Int32, b: Int32) throws -> Int32",
        "open class Counter",
        "public convenience init(start: Point)",
        "open func sum() -> Int32",
    ] {
        let compact_needle: String = needle.split_whitespace().collect();
        assert!(
            compact.contains(&compact_needle),
            "generated Swift missing `{needle}`"
        );
    }
}

/// Remove a crate's library artifacts from the shared target dir so a test can't pass on a
/// leftover from an earlier run.
fn remove_library_artifacts(lib_name: &str) {
    let dir = target_dir();
    for name in [
        format!("lib{lib_name}.dylib"),
        format!("lib{lib_name}.so"),
        format!("lib{lib_name}.a"),
        format!("lib{lib_name}.rlib"),
    ] {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

/// A crate with no `[lib] crate-type` (plain rlib) gets a dylib, a static archive, and bindings
/// from `cargo eqswift build`, and `cargo eqswift swift` builds them when they are missing.
#[test]
fn external_crate_without_crate_type() {
    let krate = write_external_crate("acme-plain", "");
    let manifest = std::fs::read_to_string(krate.path().join("Cargo.toml")).unwrap();
    assert!(
        !manifest.contains("crate-type"),
        "temp crate must not set crate-type"
    );

    let dylib = target_dir().join(format!(
        "{}acme_plain{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let archive = target_dir().join("libacme_plain.a");
    for subcommand in ["build", "swift"] {
        remove_library_artifacts("acme_plain");
        let out_dir = tempfile::tempdir().unwrap();
        let output = run_cargo_eqswift_cmd(subcommand, krate.path(), out_dir.path(), &[]);
        assert!(
            output.status.success(),
            "cargo eqswift {subcommand} should succeed for an rlib-only crate:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            dylib.exists(),
            "{subcommand}: {} should exist",
            dylib.display()
        );
        assert!(
            archive.exists(),
            "{subcommand}: {} should exist",
            archive.display()
        );
        let swift = std::fs::read_to_string(out_dir.path().join("acme_plain.swift")).unwrap();
        assert!(
            swift.contains("public func shout"),
            "{subcommand}: shout missing"
        );
        assert!(out_dir.path().join("module.modulemap").exists());
    }
}

#[test]
fn external_crate_does_not_export_eqswift_symbols() {
    let _krate = external_crate("acme-gadgets");
    let dylib = workspace_root().join("target/debug").join(format!(
        "{}acme_gadgets{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let bytes = std::fs::read(&dylib).unwrap();
    let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
    assert!(
        has(b"uniffi_acme_gadgets_fn_func_shout"),
        "user export should be present"
    );
    for needle in [
        &b"uniffi_eqswift_"[..],
        b"ffi_eqswift_",
        b"UNIFFI_META_EQSWIFT",
    ] {
        assert!(
            !has(needle),
            "user library should not contain eqswift's own UniFFI symbols ({})",
            String::from_utf8_lossy(needle)
        );
    }
}

#[test]
fn library_compiles() {
    ensure_cdylib();
}

#[test]
fn swift_bindings_generated() {
    ensure_cdylib();
    let out_dir = tempfile::tempdir().unwrap();

    let status = generate_swift(out_dir.path());
    assert!(status.success(), "uniffi-bindgen should succeed");

    let swift = out_dir.path().join("eqswift_demo.swift");
    let header = out_dir.path().join("eqswift_demoFFI.h");
    let modulemap = out_dir.path().join("eqswift_demoFFI.modulemap");

    assert!(swift.exists(), "eqswift_demo.swift should be generated");
    assert!(header.exists(), "eqswift_demoFFI.h should be generated");
    assert!(
        modulemap.exists(),
        "eqswift_demoFFI.modulemap should be generated"
    );

    let contents = std::fs::read_to_string(&swift).unwrap();
    assert!(contents.contains("public func add"), "add function missing");
    assert!(
        contents.contains("public struct Person"),
        "Person struct missing"
    );
    assert!(
        contents.contains("open class Greeter"),
        "Greeter class missing"
    );
    assert!(
        contents.contains("func greet(name: String)"),
        "greet method missing"
    );
}

#[test]
fn auto_constructor_detected() {
    ensure_cdylib();
    let out_dir = tempfile::tempdir().unwrap();

    let status = generate_swift(out_dir.path());
    assert!(status.success());

    let contents = std::fs::read_to_string(out_dir.path().join("eqswift_demo.swift")).unwrap();
    assert!(
        contents.contains("constructor"),
        "Greeter constructor should appear in generated Swift"
    );
}
