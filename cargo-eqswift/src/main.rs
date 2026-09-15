use anyhow::{bail, Context, Result};
use cargo_metadata::{Metadata, Package};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::Command;

/// cargo eqswift — zero-config Rust-to-Swift FFI
///
/// Examples:
///   cargo eqswift swift                    # generate Swift bindings (dylib)
///   cargo eqswift swift --release          # release build artifacts
///   cargo eqswift swift --static           # static link hints (libeqswift.a)
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
            static_link,
        } => generate(release, out_dir, target, link_mode(static_link)),
        EqswiftCmd::Build {
            release,
            out_dir,
            target,
            static_link,
            cargo_args,
        } => {
            let meta = cargo_metadata::MetadataCommand::new()
                .exec()
                .context("failed to run cargo metadata")?;
            let root = resolve_library_package(&meta)?;
            build(release, target.clone(), cargo_args, Some(&root.name))?;
            generate(release, out_dir, target, link_mode(static_link))
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
    link_mode: LinkMode,
) -> Result<()> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .exec()
        .context("failed to run cargo metadata")?;

    let root = resolve_library_package(&metadata)?;

    let lib_name = root
        .targets
        .iter()
        .find(|t| {
            t.kind
                .iter()
                .any(|k| k == "cdylib" || k == "dylib" || k == "staticlib" || k == "rlib")
        })
        .map(|t| t.name.as_str())
        .unwrap_or(&root.name);

    let profile = if release { "release" } else { "debug" };

    let (lib_path, resolved_mode) = find_library(
        metadata.target_directory.as_std_path(),
        lib_name,
        profile,
        target.as_deref(),
        link_mode,
    )?;

    fs_err::create_dir_all(&out_dir)?;

    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "-p",
        &root.name,
        "--bin",
        "uniffi-bindgen",
        "--",
        "generate",
        "--library",
    ]);
    cmd.arg(&lib_path);
    cmd.args(["--language", "swift", "--out-dir"]);
    cmd.arg(&out_dir);

    eprintln!("  Running: {:?}", cmd);
    let status = cmd.status().context("failed to run uniffi-bindgen")?;
    if !status.success() {
        bail!("uniffi-bindgen failed");
    }

    install_spm_modulemap(&out_dir)?;

    eprintln!("✓ Generated Swift bindings in {}", out_dir.display());
    eprintln!("  └── eqswift.swift  (and eqswiftFFI headers)");

    let lib_dir = lib_path.parent().context("library path has no parent")?;
    print_linker_hints(lib_name, lib_dir, profile, resolved_mode, target.as_deref());

    Ok(())
}

fn install_spm_modulemap(out_dir: &Path) -> Result<()> {
    let src = out_dir.join("eqswiftFFI.modulemap");
    let dst = out_dir.join("module.modulemap");
    if src.exists() && !dst.exists() {
        fs_err::copy(&src, &dst)?;
    }
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
