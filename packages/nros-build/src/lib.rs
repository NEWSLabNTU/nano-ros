//! `nros-build` — Phase 212.C build-script helper.
//!
//! Drives `nros codegen <lang>` from a downstream crate's `build.rs`.
//! Writes outputs to `$OUT_DIR/nros-gen/` only (preserves the
//! `--target-dir` isolation rule from CLAUDE.md). Skips regeneration
//! when the SHA-256 input digest matches the stamp at
//! `$OUT_DIR/nros-gen/.stamp`.
//!
//! Resolves the `nros` binary via `$NROS_BIN` → PATH → `~/.nros/bin/nros`.
//! Missing binary → hard fail with install pointer.
//!
//! Degrades to a `cargo:warning=` no-op when no RMW Cargo feature is set
//! (matches the Phase 118.B `--no-default-features` probe hazard).
//!
//! ## Example
//!
//! ```no_run
//! // build.rs
//! use nros_build::{Codegen, Lang};
//! fn main() {
//!     Codegen::new("package.xml", Lang::Rust)
//!         .feature_gate("NROS_RMW")
//!         .emit_rerun(true)
//!         .run()
//!         .expect("nros codegen");
//! }
//! ```

pub mod discovery;
pub mod nros_bin;
pub mod stamp;

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

pub use nros_bin::{MissingNrosBinary, find_nros_binary};
pub use stamp::{StampInput, compute_digest, load_stamp, save_stamp};

/// Codegen output language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    C,
    Cpp,
}

impl Lang {
    fn as_cli_arg(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::C => "c",
            Lang::Cpp => "cpp",
        }
    }
}

/// Builder for a `nros codegen` invocation.
pub struct Codegen {
    package_xml: PathBuf,
    lang: Lang,
    feature_gate_prefix: Option<String>,
    out_env: String,
    emit_rerun: bool,
}

impl Codegen {
    /// Start a new builder.
    pub fn new(package_xml: impl Into<PathBuf>, lang: Lang) -> Self {
        Self {
            package_xml: package_xml.into(),
            lang,
            feature_gate_prefix: None,
            out_env: "OUT_DIR".to_string(),
            emit_rerun: true,
        }
    }

    /// Override `package.xml` path.
    pub fn package_xml(mut self, p: impl Into<PathBuf>) -> Self {
        self.package_xml = p.into();
        self
    }

    /// Override output language.
    pub fn language(mut self, l: Lang) -> Self {
        self.lang = l;
        self
    }

    /// Set a Cargo feature-gate prefix used for no-op degradation.
    ///
    /// At build time we look for any `CARGO_FEATURE_<PREFIX>*` env var;
    /// if none is set, we emit `cargo:warning=` and skip codegen. The
    /// canonical prefix is `RMW` (matches `rmw-zenoh` / `rmw-xrce` /
    /// `rmw-cyclonedds`).
    pub fn feature_gate(mut self, prefix: &str) -> Self {
        self.feature_gate_prefix = Some(prefix.to_string());
        self
    }

    /// Override the env var read for the output directory. Defaults to
    /// `OUT_DIR` (the cargo-set build-script var).
    pub fn out_env(mut self, name: &str) -> Self {
        self.out_env = name.to_string();
        self
    }

    /// Toggle emission of `cargo:rerun-if-changed=` lines.
    pub fn emit_rerun(mut self, on: bool) -> Self {
        self.emit_rerun = on;
        self
    }

    /// Run the build step. Idempotent: skips when the input digest matches
    /// the existing stamp.
    pub fn run(self) -> Result<RunOutcome, BuildError> {
        let out_dir =
            env::var(&self.out_env).map_err(|_| BuildError::MissingEnv(self.out_env.clone()))?;
        let out_root = PathBuf::from(&out_dir).join("nros-gen");
        fs::create_dir_all(&out_root).map_err(BuildError::Io)?;

        // Feature-gate degradation: when the user wired a gate, check that
        // at least one matching CARGO_FEATURE_* env var is set.
        if let Some(prefix) = &self.feature_gate_prefix {
            if !has_any_cargo_feature(prefix) {
                println!(
                    "cargo:warning=nros-build: no `{prefix}` Cargo feature set; skipping `nros codegen` (no-op)."
                );
                return Ok(RunOutcome::SkippedNoFeature);
            }
        }

        let disc = discovery::discover(&self.package_xml).map_err(BuildError::Io)?;
        let nros = find_nros_binary().map_err(BuildError::MissingBinary)?;

        let mut args: Vec<String> = vec![
            "codegen".into(),
            self.lang.as_cli_arg().into(),
            "--package-xml".into(),
            self.package_xml.to_string_lossy().into_owned(),
            "--out-dir".into(),
            out_root.to_string_lossy().into_owned(),
        ];
        if let Some(prefix) = &self.feature_gate_prefix {
            args.push("--gate".into());
            args.push(prefix.clone());
        }

        if self.emit_rerun {
            println!("cargo:rerun-if-changed={}", self.package_xml.display());
            for f in &disc.interface_files {
                println!("cargo:rerun-if-changed={}", f.display());
            }
            if let Some(parent) = self.package_xml.parent() {
                println!("cargo:rerun-if-changed={}", parent.display());
            }
            println!("cargo:rerun-if-changed={}", nros.display());
            println!("cargo:rerun-if-env-changed=NROS_BIN");
            if let Some(prefix) = &self.feature_gate_prefix {
                println!("cargo:rerun-if-env-changed=CARGO_FEATURE_{prefix}");
            }
        }

        let mut inputs: Vec<StampInput> = Vec::with_capacity(disc.interface_files.len() + 1);
        inputs.push(StampInput {
            path: self.package_xml.clone(),
        });
        for f in &disc.interface_files {
            inputs.push(StampInput { path: f.clone() });
        }
        let digest = compute_digest(&inputs, &args).map_err(BuildError::Io)?;
        let stamp_path = out_root.join(".stamp");

        if load_stamp(&stamp_path).map_err(BuildError::Io)? == Some(digest.clone()) {
            return Ok(RunOutcome::Cached);
        }

        let status = Command::new(&nros)
            .args(&args)
            .status()
            .map_err(BuildError::Io)?;
        if !status.success() {
            return Err(BuildError::CodegenFailed { status });
        }

        save_stamp(&stamp_path, &digest).map_err(BuildError::Io)?;
        Ok(RunOutcome::Ran)
    }
}

/// Outcome of a `Codegen::run()` call.
#[derive(Debug)]
pub enum RunOutcome {
    /// Codegen ran and the stamp was refreshed.
    Ran,
    /// Stamp matched — codegen skipped.
    Cached,
    /// No RMW Cargo feature set — codegen skipped with a warning.
    SkippedNoFeature,
}

/// Errors emitted by [`Codegen::run`].
#[derive(Debug)]
pub enum BuildError {
    MissingEnv(String),
    MissingBinary(MissingNrosBinary),
    Io(std::io::Error),
    CodegenFailed { status: std::process::ExitStatus },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::MissingEnv(k) => write!(f, "env var `{k}` not set"),
            BuildError::MissingBinary(e) => write!(f, "{e}"),
            BuildError::Io(e) => write!(f, "{e}"),
            BuildError::CodegenFailed { status } => {
                write!(f, "`nros codegen` failed: {status}")
            }
        }
    }
}

impl std::error::Error for BuildError {}

/// True if at least one `CARGO_FEATURE_<PREFIX>*` env var is set.
fn has_any_cargo_feature(prefix: &str) -> bool {
    let key = format!("CARGO_FEATURE_{}", prefix.to_ascii_uppercase());
    for (k, _) in env::vars_os() {
        if let Some(k) = k.to_str() {
            if k.starts_with(&key) {
                return true;
            }
        }
    }
    false
}

#[doc(hidden)]
pub fn _out_subdir(out_dir: &Path) -> PathBuf {
    out_dir.join("nros-gen")
}
