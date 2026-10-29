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
///   cargo eqswift build                    # cargo build + generate Swift
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
}

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
            build(release, target.clone(), cargo_args, Some(&root.name))?;
            generate(
                release,
                out_dir,
                target,
                module_name,
                link_mode(static_link),
            )
        }
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

fn build(
    release: bool,
    target: Option<String>,
    extra_args: Vec<String>,
    package: Option<&str>,
) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build");
    if let Some(pkg) = package {
        cmd.args(["-p", pkg]);
    }
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
    let status = cmd.status().context("failed to run cargo build")?;
    if !status.success() {
        bail!("cargo build failed");
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

    // Rust crate name of the lib target: names the artifact (`lib{name}.dylib`) and is
    // UniFFI's default namespace, which names the generated Swift files.
    let lib_name = root
        .targets
        .iter()
        .find(|t| {
            t.kind
                .iter()
                .any(|k| k == "cdylib" || k == "dylib" || k == "staticlib" || k == "rlib")
        })
        .map(|t| t.name.as_str())
        .unwrap_or(&root.name)
        .replace('-', "_");
    let lib_name = lib_name.as_str();

    let profile = if release { "release" } else { "debug" };

    let (lib_path, resolved_mode) = find_library(
        metadata.target_directory.as_std_path(),
        lib_name,
        profile,
        target.as_deref(),
        link_mode,
    )?;

    fs_err::create_dir_all(&out_dir)?;

    let utf8 = |p: PathBuf| {
        Utf8PathBuf::from_path_buf(p).map_err(|p| anyhow!("non-UTF-8 path: {}", p.display()))
    };
    // `--module-name` is applied as a UniFFI global config override for this crate.
    let config_override = match &module_name {
        Some(name) => {
            let path = metadata.target_directory.join("eqswift-uniffi.toml");
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
    generate_bindings(GenerateOptions {
        languages: vec![TargetLanguage::Swift],
        source: utf8(lib_path.clone())?,
        out_dir: utf8(out_dir.clone())?,
        config_override,
        ..Default::default()
    })
    .context("uniffi bindgen failed")?;

    install_spm_modulemap(&out_dir, &module_name)?;

    eprintln!("✓ Generated Swift bindings in {}", out_dir.display());
    eprintln!("  └── {module_name}.swift, {module_name}FFI.h, module.modulemap");

    let lib_dir = lib_path.parent().context("library path has no parent")?;
    print_linker_hints(lib_name, lib_dir, profile, resolved_mode, target.as_deref());

    Ok(())
}

/// SwiftPM `systemLibrary` targets need the modulemap to be named `module.modulemap`.
fn install_spm_modulemap(out_dir: &Path, module_name: &str) -> Result<()> {
    let src = out_dir.join(format!("{module_name}FFI.modulemap"));
    if !src.exists() {
        bail!(
            "expected {} to be generated (does the crate call eqswift::setup!()? \
             if uniffi.toml sets a Swift module_name, pass the same --module-name)",
            src.display()
        );
    }
    fs_err::copy(&src, out_dir.join("module.modulemap"))?;
    Ok(())
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
         Run `cargo build` first.",
        lib_name,
        target_path.display()
    );
}
