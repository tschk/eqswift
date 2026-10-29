use anyhow::{anyhow, bail, Context, Result};
use cargo_metadata::camino::Utf8PathBuf;
use cargo_metadata::{Metadata, Package};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::Command;
use uniffi_bindgen::bindings::{generate as generate_bindings, GenerateOptions, TargetLanguage};

/// cargo eqswift — zero-config Rust-to-Swift FFI
///
/// Examples:
///   cargo eqswift swift                    # generate Swift bindings (dylib)
///   cargo eqswift swift --release          # release build artifacts
///   cargo eqswift swift --static           # static link hints (lib<crate>.a)
///   cargo eqswift swift --module-name Foo  # override the Swift module name
///   cargo eqswift build                    # build cdylib + staticlib, generate Swift
///   cargo eqswift xcframework              # iOS + iOS Simulator + macOS XCFramework
#[derive(Parser)]
#[command(name = "cargo-eqswift")]
#[command(bin_name = "cargo")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(name = "eqswift")]
    Eqswift {
        #[command(subcommand)]
        cmd: EqswiftCmd,
    },
}

#[derive(Subcommand)]
enum EqswiftCmd {
    Swift {
        #[arg(long)]
        release: bool,
        #[arg(long, default_value = "swift/Generated")]
        out_dir: PathBuf,
        #[arg(long)]
        target: Option<String>,
        /// Swift module name (default: the crate's lib name, e.g. `my_crate`)
        #[arg(long)]
        module_name: Option<String>,
        #[arg(
            long = "static",
            help = "Resolve lib{name}.a and print static SPM linker flags"
        )]
        static_link: bool,
    },
    Build {
        #[arg(long)]
        release: bool,
        #[arg(long, default_value = "swift/Generated")]
        out_dir: PathBuf,
        #[arg(long)]
        target: Option<String>,
        /// Swift module name (default: the crate's lib name, e.g. `my_crate`)
        #[arg(long)]
        module_name: Option<String>,
        #[arg(long = "static")]
        static_link: bool,
        #[arg(last = true)]
        cargo_args: Vec<String>,
    },
    /// Build the staticlib for iOS, the iOS Simulator and macOS (arm64) and package it as an
    /// XCFramework, with the generated Swift file next to it
    Xcframework {
        #[arg(long)]
        release: bool,
        #[arg(long, default_value = "swift/XCFramework")]
        out_dir: PathBuf,
        /// Swift module name (default: the crate's lib name, e.g. `my_crate`)
        #[arg(long)]
        module_name: Option<String>,
    },
}

/// `cargo eqswift xcframework` slices, each the Rust targets lipo'd into it. The simulator slice
/// needs x86_64 too: Xcode's generic iOS Simulator destination builds both architectures, and an
/// arm64-only slice fails to link the x86_64 half.
const XCFRAMEWORK_SLICES: [&[&str]; 3] = [
    &["aarch64-apple-ios"],
    &["aarch64-apple-ios-sim", "x86_64-apple-ios"],
    &["aarch64-apple-darwin"],
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum LinkMode {
    Dynamic,
    Static,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cmd = match cli.command {
        Commands::Eqswift { cmd } => cmd,
    };

    match cmd {
        EqswiftCmd::Swift {
            release,
            out_dir,
            target,
            module_name,
            static_link,
        } => generate(
            release,
            out_dir,
            target,
            module_name,
            link_mode(static_link),
        ),
        EqswiftCmd::Build {
            release,
            out_dir,
            target,
            module_name,
            static_link,
            cargo_args,
        } => {
            let meta = cargo_metadata::MetadataCommand::new()
                .exec()
                .context("failed to run cargo metadata")?;
            let root = resolve_library_package(&meta)?;
            build(
                release,
                target.clone(),
                cargo_args,
                Some(&root.name),
                "cdylib,staticlib",
            )?;
            generate(
                release,
                out_dir,
                target,
                module_name,
                link_mode(static_link),
            )
        }
        EqswiftCmd::Xcframework {
            release,
            out_dir,
            module_name,
        } => xcframework(release, out_dir, module_name),
    }
}

fn resolve_library_package(metadata: &Metadata) -> Result<&Package> {
    if let Some(pkg) = metadata.root_package() {
        return Ok(pkg);
    }
    metadata
        .packages
        .iter()
        .find(|p| {
            p.targets.iter().any(|t| {
                t.crate_types
                    .iter()
                    .any(|k| k == "cdylib" || k == "staticlib")
            })
        })
        .context(
            "no root package — run from a crate directory, or add a cdylib package to the workspace",
        )
}

fn link_mode(static_link: bool) -> LinkMode {
    if static_link {
        LinkMode::Static
    } else {
        LinkMode::Dynamic
    }
}

/// Build the package's lib as `crate_types` (e.g. `cdylib,staticlib`) via
/// `cargo rustc --crate-type`, so the crate needs no `[lib] crate-type` (a plain rlib works).
fn build(
    release: bool,
    target: Option<String>,
    extra_args: Vec<String>,
    package: Option<&str>,
    crate_types: &str,
) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("rustc");
    if let Some(pkg) = package {
        cmd.args(["-p", pkg]);
    }
    cmd.args(["--lib", "--crate-type", crate_types]);
    if release {
        cmd.arg("--release");
    }
    if let Some(t) = target {
        cmd.args(["--target", &t]);
    }
    for arg in extra_args {
        cmd.arg(arg);
    }

    eprintln!("  Running: {:?}", cmd);
    let status = cmd.status().context("failed to run cargo rustc")?;
    if !status.success() {
        bail!("cargo rustc failed");
    }
    Ok(())
}

fn generate(
    release: bool,
    out_dir: PathBuf,
    target: Option<String>,
    module_name: Option<String>,
    link_mode: LinkMode,
) -> Result<()> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .exec()
        .context("failed to run cargo metadata")?;

    let root = resolve_library_package(&metadata)?;

    let lib_name = lib_name(root);
    let lib_name = lib_name.as_str();

    let profile = if release { "release" } else { "debug" };

    let find = || {
        find_library(
            metadata.target_directory.as_std_path(),
            lib_name,
            profile,
            target.as_deref(),
            link_mode,
        )
    };
    let (lib_path, resolved_mode) = match find() {
        Ok(found) => found,
        Err(_) => {
            eprintln!("  No built library for '{lib_name}' yet; building it");
            build(
                release,
                target.clone(),
                Vec::new(),
                Some(&root.name),
                "cdylib,staticlib",
            )?;
            find()?
        }
    };

    let (module_name, _) = generate_swift(&metadata, lib_name, &lib_path, &out_dir, module_name)?;

    eprintln!("✓ Generated Swift bindings in {}", out_dir.display());
    eprintln!("  └── {module_name}.swift, {module_name}FFI.h, module.modulemap");

    let lib_dir = lib_path.parent().context("library path has no parent")?;
    print_linker_hints(lib_name, lib_dir, profile, resolved_mode, target.as_deref());

    Ok(())
}

/// Rust crate name of the lib target: names the artifact (`lib{name}.dylib`) and is UniFFI's
/// default namespace, which names the generated Swift files.
fn lib_name(root: &Package) -> String {
    root.targets
        .iter()
        .find(|t| {
            t.kind.iter().any(|k| {
                k == "lib" || k == "cdylib" || k == "dylib" || k == "staticlib" || k == "rlib"
            })
        })
        .map(|t| t.name.as_str())
        .unwrap_or(&root.name)
        .replace('-', "_")
}

/// Generate Swift bindings from `lib_path` into `out_dir` and install `module.modulemap`.
/// Returns the Swift module name and the FFI (C) module name.
fn generate_swift(
    metadata: &Metadata,
    lib_name: &str,
    lib_path: &Path,
    out_dir: &Path,
    module_name: Option<String>,
) -> Result<(String, String)> {
    fs_err::create_dir_all(out_dir)?;

    let utf8 = |p: PathBuf| {
        Utf8PathBuf::from_path_buf(p).map_err(|p| anyhow!("non-UTF-8 path: {}", p.display()))
    };
    // `--module-name` is applied as a UniFFI global config override for this crate. UniFFI joins
    // the name onto `out_dir`, so it must be a plain Swift identifier, and each invocation gets
    // its own override file so concurrent runs sharing a target dir cannot swap names.
    if let Some(name) = &module_name {
        if !is_swift_identifier(name) {
            bail!("--module-name must be a Swift identifier (letters, digits, `_`), got {name:?}");
        }
    }
    let config_override = match &module_name {
        Some(name) => {
            let path = metadata
                .target_directory
                .join(format!("eqswift-uniffi-{}.toml", std::process::id()));
            fs_err::write(
                &path,
                format!("[crates.{lib_name}.bindings.swift]\nmodule_name = {name:?}\n"),
            )?;
            Some(path)
        }
        None => None,
    };
    let module_name = module_name.unwrap_or_else(|| lib_name.to_string());

    eprintln!("  Generating Swift bindings from {}", lib_path.display());
    let generated = generate_bindings(GenerateOptions {
        languages: vec![TargetLanguage::Swift],
        source: utf8(lib_path.to_path_buf())?,
        out_dir: utf8(out_dir.to_path_buf())?,
        config_override: config_override.clone(),
        ..Default::default()
    })
    .context("uniffi bindgen failed");
    if let Some(path) = &config_override {
        let _ = fs_err::remove_file(path);
    }
    generated?;

    let ffi_module = install_spm_modulemap(out_dir, &module_name)?;
    Ok((module_name, ffi_module))
}

/// Build `lib<crate>.a` for every [`XCFRAMEWORK_SLICES`] target, generate the bindings once, and
/// package the archives with the FFI headers + `module.modulemap` via `xcodebuild -create-xcframework`.
fn xcframework(release: bool, out_dir: PathBuf, module_name: Option<String>) -> Result<()> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .exec()
        .context("failed to run cargo metadata")?;
    let root = resolve_library_package(&metadata)?;
    let lib_name = lib_name(root);
    let profile = if release { "release" } else { "debug" };
    let target_dir = metadata.target_directory.as_std_path();

    let mut slices = Vec::new();
    for targets in XCFRAMEWORK_SLICES {
        let mut archives = Vec::new();
        for target in targets {
            build(
                release,
                Some(target.to_string()),
                Vec::new(),
                Some(&root.name),
                "staticlib",
            )?;
            let archive = target_dir
                .join(target)
                .join(profile)
                .join(format!("lib{lib_name}.a"));
            if !archive.exists() {
                bail!("expected {} after building", archive.display());
            }
            archives.push(archive);
        }
        slices.push(archives);
    }

    let staging = target_dir.join(format!("eqswift-xcframework-{}", std::process::id()));
    let result = package_xcframework(
        &metadata,
        &lib_name,
        &slices,
        &staging,
        &out_dir,
        module_name,
    );
    let _ = std::fs::remove_dir_all(&staging);
    let (module_name, ffi_module) = result?;

    eprintln!();
    eprintln!("✓ Built XCFramework in {}", out_dir.display());
    eprintln!(
        "  ├── {ffi_module}.xcframework  (ios-arm64, ios-arm64_x86_64-simulator, macos-arm64)"
    );
    eprintln!("  └── {module_name}.swift");
    eprintln!();
    eprintln!("Use it from SwiftPM (copy both into your package):");
    eprintln!(
        "  .binaryTarget(name: \"{ffi_module}\", path: \"Frameworks/{ffi_module}.xcframework\"),"
    );
    eprintln!(
        "  .target(name: \"{module_name}\", dependencies: [\"{ffi_module}\"], path: \"Sources/{module_name}\"),  // {module_name}.swift"
    );
    eprintln!("Then depend on \"{module_name}\" and `import {module_name}`.");
    eprintln!("In Xcode, add {ffi_module}.xcframework to the target and {module_name}.swift to its sources.");
    Ok(())
}

/// Bindings are target-independent, so they come from the macOS archive (the last slice).
/// The headers go in `Headers/<FFI module>/` so several XCFrameworks' `module.modulemap` files
/// don't collide when Xcode copies headers into one include dir.
fn package_xcframework(
    metadata: &Metadata,
    lib_name: &str,
    slices: &[Vec<PathBuf>],
    staging: &Path,
    out_dir: &Path,
    module_name: Option<String>,
) -> Result<(String, String)> {
    let _ = std::fs::remove_dir_all(staging);
    let bindings = staging.join("bindings");
    let macos_archive = slices
        .last()
        .and_then(|archives| archives.first())
        .context("no archives built")?;
    let (module_name, ffi_module) =
        generate_swift(metadata, lib_name, macos_archive, &bindings, module_name)?;

    let headers = staging.join("headers");
    let module_headers = headers.join(&ffi_module);
    fs_err::create_dir_all(&module_headers)?;
    let modulemap = fs_err::read_to_string(bindings.join("module.modulemap"))?;
    fs_err::write(module_headers.join("module.modulemap"), &modulemap)?;
    // Copy the header(s) the modulemap names.
    for line in modulemap.lines() {
        if let Some(header) = line.trim().strip_prefix("header \"") {
            let header = header.trim_end_matches('"');
            fs_err::copy(bindings.join(header), module_headers.join(header))?;
        }
    }

    fs_err::create_dir_all(out_dir)?;
    let output = out_dir.join(format!("{ffi_module}.xcframework"));
    if output.exists() {
        fs_err::remove_dir_all(&output)?;
    }
    let mut cmd = Command::new("xcodebuild");
    cmd.arg("-create-xcframework");
    for (index, archives) in slices.iter().enumerate() {
        let archive = match archives.as_slice() {
            [single] => single.clone(),
            several => {
                let fat = staging
                    .join(format!("lipo-{index}"))
                    .join(format!("lib{lib_name}.a"));
                fs_err::create_dir_all(fat.parent().unwrap())?;
                let status = Command::new("lipo")
                    .arg("-create")
                    .args(several)
                    .arg("-output")
                    .arg(&fat)
                    .status()
                    .context("failed to run lipo")?;
                if !status.success() {
                    bail!("lipo -create failed for {several:?}");
                }
                fat
            }
        };
        cmd.arg("-library")
            .arg(&archive)
            .arg("-headers")
            .arg(&headers);
    }
    cmd.arg("-output").arg(&output);
    eprintln!("  Running: {:?}", cmd);
    let status = cmd
        .status()
        .context("failed to run xcodebuild (is Xcode installed?)")?;
    if !status.success() {
        bail!("xcodebuild -create-xcframework failed");
    }

    let swift = format!("{module_name}.swift");
    fs_err::copy(bindings.join(&swift), out_dir.join(&swift))?;
    Ok((module_name, ffi_module))
}

/// SwiftPM `systemLibrary` targets need the modulemap to be named `module.modulemap`.
///
/// The FFI module is the one `<module_name>.swift` imports (`#if canImport(X)`), and its modulemap
/// is the generated one declaring `module X`, so a `uniffi.toml` that sets `ffi_module_name` or
/// `ffi_module_filename` is followed rather than assuming `<module_name>FFI.modulemap`.
fn install_spm_modulemap(out_dir: &Path, module_name: &str) -> Result<String> {
    let swift = out_dir.join(format!("{module_name}.swift"));
    let ffi_module = fs_err::read_to_string(&swift)
        .ok()
        .and_then(|source| {
            let rest = source.split_once("#if canImport(")?.1;
            Some(rest.split_once(')')?.0.trim().to_string())
        })
        .with_context(|| {
            format!(
                "expected {} to be generated (does the crate call eqswift::setup!()? \
                 if uniffi.toml sets a Swift module_name, pass the same --module-name)",
                swift.display()
            )
        })?;
    let declaration = format!("module {ffi_module} {{");
    let src = fs_err::read_dir(out_dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "modulemap")
                && path
                    .file_name()
                    .is_some_and(|name| name != "module.modulemap")
        })
        .find(|path| std::fs::read_to_string(path).is_ok_and(|text| text.contains(&declaration)))
        .with_context(|| format!("no generated modulemap declares `module {ffi_module}`"))?;
    fs_err::copy(&src, out_dir.join("module.modulemap"))?;
    Ok(ffi_module)
}

fn is_swift_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn print_linker_hints(
    lib_name: &str,
    lib_dir: &Path,
    profile: &str,
    link_mode: LinkMode,
    target: Option<&str>,
) {
    let dir = lib_dir.display();
    eprintln!();
    eprintln!("Linker (SPM Package.swift or Xcode):");
    eprintln!("  profile: {profile}");
    if let Some(t) = target {
        eprintln!("  target:  {t}");
    }
    eprintln!("  rust lib dir: {}", dir);
    match link_mode {
        LinkMode::Dynamic => {
            eprintln!("  dynamic: .unsafeFlags([\"-L\", \"{dir}\", \"-l{lib_name}\"])");
            eprintln!("  run (macOS dev): export DYLD_LIBRARY_PATH=\"{dir}:$DYLD_LIBRARY_PATH\"");
        }
        LinkMode::Static => {
            let archive = lib_dir.join(format!("lib{lib_name}.a"));
            eprintln!(
                "  static:  .unsafeFlags([\"-L\", \"{dir}\", \"-force_load\", \"{}\"])",
                archive.display()
            );
            eprintln!("  bindgen still uses cdylib/dylib metadata when present; built staticlib for app link.");
        }
    }
    eprintln!("  env overrides for swift/Package.swift: EQSWIFT_PROFILE, EQSWIFT_STATIC=1");
}

fn find_library(
    target_dir: &Path,
    lib_name: &str,
    profile: &str,
    target: Option<&str>,
    prefer: LinkMode,
) -> Result<(PathBuf, LinkMode)> {
    let target_path = match target {
        Some(t) => target_dir.join(t).join(profile),
        None => target_dir.join(profile),
    };

    let static_candidates = [format!("lib{lib_name}.a"), format!("{lib_name}.lib")];
    let dynamic_candidates = [
        format!("lib{lib_name}.dylib"),
        format!("lib{lib_name}.so"),
        format!("{lib_name}.dll"),
    ];

    let try_order: &[LinkMode] = match prefer {
        LinkMode::Static => &[LinkMode::Static, LinkMode::Dynamic],
        LinkMode::Dynamic => &[LinkMode::Dynamic, LinkMode::Static],
    };

    for mode in try_order {
        let names = match mode {
            LinkMode::Static => &static_candidates[..],
            LinkMode::Dynamic => &dynamic_candidates[..],
        };
        for candidate in names {
            let path = target_path.join(candidate);
            if path.exists() {
                return Ok((path, *mode));
            }
        }
    }

    bail!(
        "could not find compiled library for '{}' in {} (tried static and dynamic). \
         Run `cargo eqswift build` first.",
        lib_name,
        target_path.display()
    );
}
