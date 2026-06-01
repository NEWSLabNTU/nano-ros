//! Phase 212.K.4 — Cyclone-DDS descriptor codegen helper for `build.rs`.
//!
//! Drives `nros codegen cyclonedds-descriptors` from a downstream
//! crate's build script and returns the emitted artifacts so the
//! caller can feed them into `cc::Build`. The companion K.2 baked-in
//! defaults (`std_msgs/Int32`, `rmw_dds_common_graph`) stay as universal
//! fallbacks — this helper layers per-example descriptors on top.
//!
//! ## Example
//!
//! ```ignore
//! use nros_build::cyclonedds::Descriptors;
//! fn main() {
//!     let emitted = Descriptors::new()
//!         .messages(&["std_msgs/Int32"])
//!         .emit()
//!         .expect("nros codegen cyclonedds-descriptors");
//!     cc::Build::new()
//!         .include(&emitted.include_dir)
//!         .files(emitted.generated_c)
//!         .compile("example_descriptors");
//!     println!(
//!         "cargo:rustc-link-lib=static:+whole-archive,-bundle=example_descriptors"
//!     );
//! }
//! ```

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};

use crate::{BuildError, find_nros_binary};

/// One emitted descriptor entry. Mirror of the manifest entry written
/// by `nros codegen cyclonedds-descriptors`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub pkg: String,
    pub msg: String,
    pub idl_path: PathBuf,
    pub c_path: PathBuf,
    pub h_path: PathBuf,
    pub type_name: String,
    pub descriptor_symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    crate_name: String,
    register_entry: String,
    register_c: PathBuf,
    register_h: PathBuf,
    descriptors: Vec<ManifestEntry>,
}

/// Builder for a `nros codegen cyclonedds-descriptors` invocation.
pub struct Descriptors {
    messages: Vec<String>,
    msg_search_paths: Vec<PathBuf>,
    crate_name: Option<String>,
}

impl Default for Descriptors {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            msg_search_paths: Vec::new(),
            crate_name: None,
        }
    }
}

impl Descriptors {
    pub fn new() -> Self {
        Self::default()
    }

    /// `<pkg>/<Msg>` entries to bake.
    pub fn messages<S: AsRef<str>>(mut self, msgs: &[S]) -> Self {
        for m in msgs {
            self.messages.push(m.as_ref().to_string());
        }
        self
    }

    /// Append a `.msg` search root. The helper resolves each
    /// `<pkg>/<Msg>` entry against `<root>/<pkg>/msg/<Msg>.msg` in
    /// the order roots were registered, before falling back to the
    /// bundled `rcl-interfaces` tree under the repo root.
    pub fn msg_search_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.msg_search_paths.push(path.into());
        self
    }

    /// Override the crate / register-entry prefix. Defaults to the
    /// build-script's `CARGO_PKG_NAME` lowered to a C identifier.
    pub fn crate_name(mut self, name: impl Into<String>) -> Self {
        self.crate_name = Some(name.into());
        self
    }

    /// Run the verb and return the emitted artifact set.
    pub fn emit(self) -> Result<EmittedDescriptors, BuildError> {
        let out_root = PathBuf::from(
            env::var("OUT_DIR").map_err(|_| BuildError::MissingEnv("OUT_DIR".into()))?,
        )
        .join("cyclonedds-descriptors");
        fs::create_dir_all(&out_root).map_err(BuildError::Io)?;

        if self.messages.is_empty() {
            // Caller wired the helper but registered no messages — emit
            // an empty manifest so the cc::Build invocation downstream
            // is a no-op rather than failing.
            return Ok(EmittedDescriptors {
                generated_c: Vec::new(),
                include_dir: out_root,
                register_entry: String::new(),
                manifest_path: PathBuf::new(),
                entries: Vec::new(),
            });
        }

        let nros = find_nros_binary().map_err(BuildError::MissingBinary)?;
        let idlc = env::var_os("DEP_DDSC_IDLC").ok_or_else(|| {
            BuildError::MissingEnv("DEP_DDSC_IDLC (add `cyclonedds-sys` to your build-deps)".into())
        })?;
        let include = env::var_os("DEP_DDSC_INCLUDE").ok_or_else(|| {
            BuildError::MissingEnv(
                "DEP_DDSC_INCLUDE (add `cyclonedds-sys` to your build-deps)".into(),
            )
        })?;
        println!("cargo:rerun-if-env-changed=DEP_DDSC_IDLC");
        println!("cargo:rerun-if-env-changed=DEP_DDSC_INCLUDE");
        println!("cargo:rerun-if-env-changed=NROS_BIN");

        let crate_name = self
            .crate_name
            .or_else(|| env::var("CARGO_PKG_NAME").ok())
            .unwrap_or_else(|| "nros_descriptors".to_string());

        let mut cmd = Command::new(&nros);
        cmd.args(["codegen", "cyclonedds-descriptors"])
            .arg("--idlc")
            .arg(&idlc)
            .arg("--include")
            .arg(&include)
            .arg("--crate-name")
            .arg(&crate_name)
            .arg("--out")
            .arg(&out_root);

        for raw in &self.messages {
            let source = resolve_msg_source(raw, &self.msg_search_paths)?;
            println!("cargo:rerun-if-changed={}", source.display());
            cmd.arg("--msg").arg(format!("{raw}={}", source.display()));
        }

        let status = cmd.status().map_err(BuildError::Io)?;
        if !status.success() {
            return Err(BuildError::CodegenFailed { status });
        }

        let manifest_path = out_root.join("cyclonedds-descriptors.json");
        let body = fs::read_to_string(&manifest_path).map_err(BuildError::Io)?;
        let manifest: Manifest =
            serde_json::from_str(&body).map_err(|e| BuildError::Io(std::io::Error::other(e)))?;

        let mut generated_c: Vec<PathBuf> = manifest
            .descriptors
            .iter()
            .map(|d| d.c_path.clone())
            .collect();
        generated_c.push(manifest.register_c.clone());

        Ok(EmittedDescriptors {
            generated_c,
            include_dir: out_root,
            register_entry: manifest.register_entry,
            manifest_path,
            entries: manifest.descriptors,
        })
    }
}

/// Outcome of [`Descriptors::emit`].
#[derive(Debug, Clone)]
pub struct EmittedDescriptors {
    /// Every C source the caller should compile (idlc `.c` + register TU).
    pub generated_c: Vec<PathBuf>,
    /// Header search dir to feed into `cc::Build::include`.
    pub include_dir: PathBuf,
    /// `extern "C" void <name>(void)` symbol the consumer may call
    /// explicitly (also pulled in via the constructor hook).
    pub register_entry: String,
    /// Path to the JSON manifest (mostly useful for tests / debugging).
    pub manifest_path: PathBuf,
    /// One entry per `<pkg>/<Msg>` baked.
    pub entries: Vec<ManifestEntry>,
}

fn resolve_msg_source(spec: &str, search_paths: &[PathBuf]) -> Result<PathBuf, BuildError> {
    let (pkg, msg) = spec.split_once('/').ok_or_else(|| {
        BuildError::Io(std::io::Error::other(format!(
            "cyclonedds::Descriptors: `--msg` expects `<pkg>/<Msg>`, got {spec:?}"
        )))
    })?;
    let rel = Path::new(pkg).join("msg").join(format!("{msg}.msg"));
    for root in search_paths {
        let candidate = root.join(&rel);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(BuildError::Io(std::io::Error::other(format!(
        "cyclonedds::Descriptors: `.msg` for {spec} not found in any registered search root \
         (tried {} root(s)); add one via `Descriptors::msg_search_path(...)`",
        search_paths.len()
    ))))
}

// Tests for the resolve / emit pipeline live in
// `packages/testing/nros-tests/tests/phase212_k4_cyclonedds_descriptors.rs`.
// Kept out of the per-crate src/ tree so the Phase 212.C 500-LoC budget
// fits the helper proper.
