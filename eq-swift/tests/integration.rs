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
/// the way a user would. Builds share this repo's `target/` dir.
fn external_crate(package_name: &str) -> tempfile::TempDir {
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

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
eqswift = {{ path = {eqswift_path:?} }}
uniffi = "0.32"
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
"#,
    )
    .unwrap();

    let status = Command::new("cargo")
        .args(["build", "-j2", "--quiet"])
        .current_dir(dir.path())
        .env("CARGO_TARGET_DIR", workspace_root().join("target"))
        .status()
        .expect("cargo build should run");
    assert!(status.success(), "external crate should build");
    dir
}

/// Run `cargo eqswift swift` (from this workspace's sources) inside `crate_dir`.
fn cargo_eqswift_swift(crate_dir: &std::path::Path, extra_args: &[&str]) -> tempfile::TempDir {
    let out_dir = tempfile::tempdir().unwrap();
    let output = Command::new("cargo")
        .args(["run", "-j2", "--quiet", "--manifest-path"])
        .arg(workspace_root().join("Cargo.toml"))
        .args(["-p", "cargo-eqswift", "--", "eqswift", "swift", "--out-dir"])
        .arg(out_dir.path())
        .args(extra_args)
        .current_dir(crate_dir)
        .env("CARGO_TARGET_DIR", workspace_root().join("target"))
        .output()
        .expect("cargo eqswift should run");
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
