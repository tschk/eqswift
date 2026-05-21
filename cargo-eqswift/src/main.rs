use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::Command;

/// cargo eqswift — zero-config Rust-to-Swift FFI
///
/// Examples:
///   cargo eqswift swift                    # generate Swift bindings
///   cargo eqswift swift --release          # use release build
///   cargo eqswift build                    # build + generate Swift
///   cargo eqswift kotlin --out-dir ./out   # generate Kotlin bindings
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
    /// Generate foreign-language bindings
    #[command(name = "eqswift")]
    Eqswift {
        #[command(subcommand)]
        cmd: EqswiftCmd,
    },
}

#[derive(Subcommand)]
enum EqswiftCmd {
    /// Generate Swift bindings
    Swift {
        /// Build in release mode
        #[arg(long)]
        release: bool,
        /// Output directory for generated bindings
        #[arg(long, default_value = "swift/Generated")]
        out_dir: PathBuf,
        /// Target triple (e.g. aarch64-apple-darwin)
        #[arg(long)]
        target: Option<String>,
    },
    /// Generate Kotlin bindings
    Kotlin {
        /// Build in release mode
        #[arg(long)]
        release: bool,
        /// Output directory for generated bindings
        #[arg(long, default_value = "kotlin/Generated")]
        out_dir: PathBuf,
        /// Target triple
        #[arg(long)]
        target: Option<String>,
    },
    /// Generate Python bindings
    Python {
        /// Build in release mode
        #[arg(long)]
        release: bool,
        /// Output directory for generated bindings
        #[arg(long, default_value = "python/Generated")]
        out_dir: PathBuf,
        /// Target triple
        #[arg(long)]
        target: Option<String>,
    },
    /// Build the Rust library and generate Swift bindings
    Build {
        /// Build in release mode
        #[arg(long)]
        release: bool,
        /// Output directory for generated bindings
        #[arg(long, default_value = "swift/Generated")]
        out_dir: PathBuf,
        /// Target triple
        #[arg(long)]
        target: Option<String>,
        /// Extra arguments to pass to cargo build
        #[arg(last = true)]
        cargo_args: Vec<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Language {
    Swift,
    Kotlin,
    Python,
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
        } => generate(Language::Swift, release, out_dir, target),
        EqswiftCmd::Kotlin {
            release,
            out_dir,
            target,
        } => generate(Language::Kotlin, release, out_dir, target),
        EqswiftCmd::Python {
            release,
            out_dir,
            target,
        } => generate(Language::Python, release, out_dir, target),
        EqswiftCmd::Build {
            release,
            out_dir,
            target,
            cargo_args,
        } => {
            build(release, target.clone(), cargo_args)?;
            generate(Language::Swift, release, out_dir, target)
        }
    }
}

fn build(release: bool, target: Option<String>, extra_args: Vec<String>) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build");
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

fn generate(lang: Language, release: bool, out_dir: PathBuf, target: Option<String>) -> Result<()> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .exec()
        .context("failed to run cargo metadata")?;

    let root = metadata
        .root_package()
        .context("no root package found — run from a cargo project root")?;

    // Find the library target name (may differ from package name)
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

    let lib_path = find_library(
        metadata.target_directory.as_std_path(),
        lib_name,
        profile,
        target.as_deref(),
    )?;

    fs_err::create_dir_all(&out_dir)?;

    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "--bin",
        "uniffi-bindgen",
        "--",
        "generate",
        "--library",
    ]);
    cmd.arg(&lib_path);
    cmd.args(["--language", lang_flag(lang), "--out-dir"]);
    cmd.arg(&out_dir);

    eprintln!("  Running: {:?}", cmd);
    let status = cmd.status().context("failed to run uniffi-bindgen")?;
    if !status.success() {
        bail!("uniffi-bindgen failed");
    }

    let ext = match lang {
        Language::Swift => "swift",
        Language::Kotlin => "kt",
        Language::Python => "py",
    };
    eprintln!(
        "✓ Generated {} bindings in {}",
        lang_name(lang),
        out_dir.display()
    );
    eprintln!("  └── {}.{}  (and FFI headers)", lib_name, ext);

    Ok(())
}

fn find_library(
    target_dir: &Path,
    lib_name: &str,
    profile: &str,
    target: Option<&str>,
) -> Result<PathBuf> {
    let target_path = match target {
        Some(t) => target_dir.join(t).join(profile),
        None => target_dir.join(profile),
    };

    let candidates = [
        format!("lib{}.dylib", lib_name),
        format!("lib{}.so", lib_name),
        format!("{}.dll", lib_name),
    ];

    for candidate in &candidates {
        let path = target_path.join(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    bail!(
        "could not find compiled library for '{}' in {}. \
         Did you run `cargo build` first?",
        lib_name,
        target_path.display()
    );
}

fn lang_flag(lang: Language) -> &'static str {
    match lang {
        Language::Swift => "swift",
        Language::Kotlin => "kotlin",
        Language::Python => "python",
    }
}

fn lang_name(lang: Language) -> &'static str {
    match lang {
        Language::Swift => "Swift",
        Language::Kotlin => "Kotlin",
        Language::Python => "Python",
    }
}
