//! Integration smoke test — build library, generate Swift bindings, verify output.

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
    target_dir().join(format!("libeqswift{}", std::env::consts::DLL_SUFFIX))
}

/// `cargo test` links the rlib; UniFFI bindgen needs the cdylib/dylib artifact.
fn ensure_cdylib() {
    let status = Command::new("cargo")
        .args(["build", "-p", "eqswift", "--quiet"])
        .status()
        .expect("cargo build -p eqswift should run");
    assert!(status.success(), "cargo build -p eqswift should succeed");
    let dylib = library_path();
    assert!(dylib.exists(), "library should exist: {}", dylib.display());
}

fn generate_swift(out_dir: &std::path::Path) -> std::process::ExitStatus {
    Command::new("cargo")
        .args([
            "run",
            "-p",
            "eqswift",
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

    let swift = out_dir.path().join("eqswift.swift");
    let header = out_dir.path().join("eqswiftFFI.h");
    let modulemap = out_dir.path().join("eqswiftFFI.modulemap");

    assert!(swift.exists(), "eqswift.swift should be generated");
    assert!(header.exists(), "eqswiftFFI.h should be generated");
    assert!(
        modulemap.exists(),
        "eqswiftFFI.modulemap should be generated"
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

    let contents = std::fs::read_to_string(out_dir.path().join("eqswift.swift")).unwrap();
    assert!(
        contents.contains("constructor"),
        "Greeter constructor should appear in generated Swift"
    );
}
