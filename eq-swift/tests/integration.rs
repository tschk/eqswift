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

#[test]
fn library_compiles() {
    // The fact that this test binary exists means the library compiled.
    // This is a trivial sanity check.
    let dylib = target_dir().join(format!("libeqswift{}", std::env::consts::DLL_SUFFIX));
    assert!(dylib.exists(), "library should exist: {}", dylib.display());
}

#[test]
fn swift_bindings_generated() {
    let out_dir = tempfile::tempdir().unwrap();

    let status = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "uniffi-bindgen",
            "--",
            "generate",
            "--library",
        ])
        .arg(target_dir().join(format!("libeqswift{}", std::env::consts::DLL_SUFFIX)))
        .args(["--language", "swift", "--out-dir"])
        .arg(out_dir.path())
        .status()
        .expect("uniffi-bindgen should run");

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
    let out_dir = tempfile::tempdir().unwrap();

    let status = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "uniffi-bindgen",
            "--",
            "generate",
            "--library",
        ])
        .arg(target_dir().join(format!("libeqswift{}", std::env::consts::DLL_SUFFIX)))
        .args(["--language", "swift", "--out-dir"])
        .arg(out_dir.path())
        .status()
        .expect("uniffi-bindgen should run");

    assert!(status.success());

    let contents = std::fs::read_to_string(out_dir.path().join("eqswift.swift")).unwrap();
    // Constructor should be present (detected from fn new() -> Self)
    assert!(
        contents.contains("constructor"),
        "auto-constructor should be detected"
    );
}
