//! Phase 172.E (driver) — metadata-mode build + run.
//!
//! Produces a component's `source-metadata.json` by compiling a tiny **host**
//! harness that links the component crate, runs its `Component::register`
//! against the in-memory `MetadataRecorder` (no transport, no RTOS task), and
//! serializes the recorder via `to_source_metadata_json`. This is the "compile
//! each component in a host-side metadata mode and invoke its entry path with a
//! fake `ComponentContext`" step from the workflow design — the input `nros
//! metadata` collects + the planner consumes.
//!
//! Scope (chosen 2026-05-28): the **driver** only. Hardening this execution
//! (resource limits, fs/network sandbox for untrusted component crates) is the
//! deferred 172.E sandbox; it wraps the `cargo` invocation here when it lands.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use eyre::{Result, WrapErr, bail, eyre};

#[derive(Debug, Clone)]
pub struct MetadataBuildOptions {
    /// Component id (e.g. `demo_pkg::talker`, or a bare `talker` for the
    /// Cargo `[package.metadata.nros.node]` shape). Diagnostics + probe dir
    /// naming; the Rust identity comes from `class` / `crate_name`.
    pub component_id: String,
    /// phase-307 W1 — the registered type's fully qualified path, verbatim
    /// from the manifest's `class`. `None` keeps the legacy
    /// `<crate>::<module>::Component` derivation from `component_id`.
    pub class: Option<String>,
    /// phase-307 W1 — rustc-visible crate name for the harness path dep.
    /// `None` falls back to `component_id`'s first `::` segment.
    pub crate_name: Option<String>,
    /// ROS package name (the `package` field of the emitted metadata).
    pub package: String,
    /// Component name (`Component::NAME`).
    pub component: String,
    pub executable: Option<String>,
    pub exported_symbol: Option<String>,
    /// The component crate directory (a Cargo path dependency of the harness).
    pub component_dir: PathBuf,
    /// nano-ros workspace root (for the `nros` path dependency).
    pub nano_ros_workspace: PathBuf,
    /// Where the harness writes the source-metadata JSON.
    pub output_path: PathBuf,
    /// Scratch directory for the generated harness crate.
    pub harness_dir: PathBuf,
}

/// The registered type's path.
///
/// A declared `class` is authoritative: the shipping `nros::node!(Class)` shape
/// is `impl Node for Class` in the crate root, so the historical positional
/// guess below (`crate::module::Component`) names a type that does not exist
/// and the harness fails to compile. The guess survives only as the fallback
/// for legacy `crate::module` component manifests, where it IS the convention.
fn component_type_path(o: &MetadataBuildOptions) -> Option<String> {
    if let Some(class) = o.class.as_deref().filter(|c| !c.is_empty()) {
        return Some(class.to_string());
    }
    let mut parts = o.component_id.split("::").filter(|p| !p.is_empty());
    let krate = parts.next()?;
    let module = parts.next()?;
    Some(format!("{krate}::{module}::Component"))
}

fn crate_name(o: &MetadataBuildOptions) -> Option<&str> {
    if let Some(name) = o.crate_name.as_deref().filter(|n| !n.is_empty()) {
        return Some(name);
    }
    // A declared class is `<crate>::<Type>`; the legacy id is `<crate>::<module>`.
    // Either way the crate is the first segment.
    o.class
        .as_deref()
        .unwrap_or(&o.component_id)
        .split("::")
        .next()
        .filter(|s| !s.is_empty())
}

pub fn render_harness_cargo_toml(o: &MetadataBuildOptions) -> Result<String> {
    let krate = crate_name(o)
        .ok_or_else(|| eyre!("component id '{}' has no crate segment", o.component_id))?;
    // `[workspace]` — the harness is generated into an arbitrary scratch dir;
    // without its own (empty) workspace table cargo walks up and captures it
    // into whatever workspace encloses that dir ("current package believes
    // it's in a workspace when it's not") — e.g. a user running `nros
    // metadata --build` anywhere under a cargo workspace (issue #202 triage).
    Ok(format!(
        "[package]\n\
         name = \"nros-metadata-probe\"\n\
         version = \"0.0.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\n\
         [workspace]\n\n\
         [[bin]]\n\
         name = \"probe\"\n\
         path = \"src/main.rs\"\n\n\
         [dependencies]\n\
         nros = {{ path = {nros:?}, features = [\"std\"] }}\n\
         {krate} = {{ path = {comp:?}, package = {pkg:?} }}\n",
        nros = o
            .nano_ros_workspace
            .join("packages/core/nros")
            .display()
            .to_string(),
        comp = o.component_dir.display().to_string(),
        pkg = cargo_package_name(&o.component_dir).unwrap_or_else(|| krate.to_string()),
    ))
}

/// The component crate's REAL Cargo package name, read from its manifest.
///
/// The harness dep must be keyed by the rustc-visible crate name (so
/// `use <krate>::…` in `main.rs` resolves) but a path dependency is looked up
/// by PACKAGE name, and the two differ whenever an example is named with
/// hyphens (`qemu-rtic-action-client` → crate `qemu_rtic_action_client`). Cargo
/// resolves that with the `package = "…"` rename field; without it the build
/// fails "no matching package named `qemu_rtic_action_client` found".
///
/// Falls back to the crate name when the manifest can't be read — the
/// underscored-name case, which is the majority and was already working.
fn cargo_package_name(component_dir: &Path) -> Option<String> {
    let manifest = std::fs::read_to_string(component_dir.join("Cargo.toml")).ok()?;
    let parsed: toml::Value = manifest.parse().ok()?;
    parsed
        .get("package")?
        .get("name")?
        .as_str()
        .map(str::to_string)
}

pub fn render_harness_main(o: &MetadataBuildOptions) -> Result<String> {
    let type_path = component_type_path(o).ok_or_else(|| {
        eyre!(
            "component '{}' declares no `class` and its id is not `crate::module` \
             — cannot name the registered type",
            o.component_id
        )
    })?;
    let exe = o
        .executable
        .as_deref()
        .map(|e| format!("\n        .executable({e:?})"))
        .unwrap_or_default();
    let sym = o
        .exported_symbol
        .as_deref()
        .map(|s| format!("\n        .exported_symbol({s:?})"))
        .unwrap_or_default();
    Ok(format!(
        "// Generated metadata-mode harness (Phase 172.E). Records {type_path}'s\n\
         // declarations against an in-memory recorder; opens no transport.\n\
         fn main() {{\n\
         \x20   // Bare type ⇒ default capacity const-params.\n\
         \x20   let mut recorder: nros::MetadataRecorder = nros::MetadataRecorder::default();\n\
         \x20   nros::record_node_metadata::<{type_path}>(&mut recorder)\n\
         \x20       .expect(\"component register (metadata mode)\");\n\
         \x20   let export = nros::SourceMetadataExport::new({pkg:?}, {comp:?}){exe}{sym};\n\
         \x20   let json = recorder\n\
         \x20       .to_source_metadata_json(&export)\n\
         \x20       .expect(\"serialize source metadata\");\n\
         \x20   std::fs::write({out:?}, json).expect(\"write source metadata\");\n\
         }}\n",
        pkg = o.package,
        comp = o.component,
        out = o.output_path.display().to_string(),
    ))
}

/// Generate the harness crate, then `cargo run` it so it writes the
/// source-metadata JSON to `output_path`.
pub fn build_metadata(o: &MetadataBuildOptions) -> Result<()> {
    let src = o.harness_dir.join("src");
    std::fs::create_dir_all(&src).wrap_err_with(|| format!("create {}", src.display()))?;
    write_if_changed(
        &o.harness_dir.join("Cargo.toml"),
        &render_harness_cargo_toml(o)?,
    )?;
    write_if_changed(&src.join("main.rs"), &render_harness_main(o)?)?;
    if let Some(parent) = o.output_path.parent() {
        std::fs::create_dir_all(parent).wrap_err_with(|| format!("create {}", parent.display()))?;
    }

    // The probe is a HOST binary — that is the whole reason one probe covers
    // every deploy target. But a standalone embedded example's
    // `.cargo/config.toml` sets `[build] target = "thumbv7m-none-eabi"`, and
    // the harness inherits it through cargo's config walk-up (which it must,
    // for the `[patch.crates-io]` entries). Without an explicit `--target` the
    // probe cross-compiles for the board and dies on `can't find crate for
    // std`. An explicit flag beats config, so name the host triple.
    let host = host_triple();
    let manifest = o.harness_dir.join("Cargo.toml");
    let target_dir = o.harness_dir.join("target");
    let status = Command::new("cargo")
        // phase-307 W1 — cargo discovers `.cargo/config.toml` by walking up
        // from its CWD, and a Node pkg's generated interface deps
        // (`example_interfaces = { version = "*" }` and friends) exist ONLY as
        // `[patch.crates-io]` entries in the consuming workspace's config. Run
        // from the harness dir (which the caller places inside the workspace)
        // so those patches are in scope; inheriting the invoking cwd resolved
        // the interface crates against crates.io and failed with "no matching
        // package named `example_interfaces` found".
        .current_dir(&o.harness_dir)
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--target")
        .arg(&host)
        .arg("--target-dir")
        .arg(&target_dir)
        // The harness inherits no pinned toolchain so a generated
        // `rust-toolchain.toml` elsewhere can't force a re-resolve.
        .env_remove("RUSTUP_TOOLCHAIN")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .wrap_err_with(|| format!("run metadata-mode harness for '{}'", o.component_id))?;
    if !status.success() {
        bail!(
            "metadata-mode harness failed (exit {}) for component '{}'",
            status.code().unwrap_or(-1),
            o.component_id
        );
    }
    if !o.output_path.is_file() {
        bail!(
            "metadata-mode harness produced no source metadata at {}",
            o.output_path.display()
        );
    }
    Ok(())
}

/// The host target triple, from `rustc -vV`.
///
/// Falls back to no explicit target when rustc cannot be read; a workspace
/// whose config sets no `[build] target` then behaves exactly as before.
fn host_triple() -> String {
    let out = Command::new("rustc").arg("-vV").output();
    if let Ok(out) = out
        && let Ok(text) = String::from_utf8(out.stdout)
        && let Some(line) = text.lines().find(|l| l.starts_with("host: "))
    {
        return line["host: ".len()..].trim().to_string();
    }
    // Best-effort default; the common host in this repo's CI + dev images.
    "x86_64-unknown-linux-gnu".to_string()
}

fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(());
    }
    std::fs::write(path, contents).wrap_err_with(|| format!("write {}", path.display()))
}


#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> MetadataBuildOptions {
        MetadataBuildOptions {
            component_id: "demo_pkg::talker".into(),
            class: None,
            crate_name: None,
            package: "demo_pkg".into(),
            component: "talker".into(),
            executable: Some("talker".into()),
            exported_symbol: Some("nros_component_talker".into()),
            component_dir: PathBuf::from("/ws/src/demo_pkg"),
            nano_ros_workspace: PathBuf::from("/nano-ros"),
            output_path: PathBuf::from("/out/talker.metadata.json"),
            harness_dir: PathBuf::from("/scratch/probe"),
        }
    }

    #[test]
    fn type_path_and_crate_name_legacy_positional_guess() {
        let o = opts();
        assert_eq!(
            component_type_path(&o).as_deref(),
            Some("demo_pkg::talker::Component")
        );
        assert_eq!(crate_name(&o), Some("demo_pkg"));
        let mut bare = opts();
        bare.component_id = "nocrate".into();
        assert_eq!(component_type_path(&bare), None); // needs crate::module
    }

    /// phase-307 W1 — the shipping `nros::node!(Class)` shape: a declared class
    /// names the type directly, and the crate name is carried, not guessed from
    /// a component id that no longer contains it.
    #[test]
    fn declared_class_wins_over_the_positional_guess() {
        let mut o = opts();
        o.component_id = "fibonacci_client".into();
        o.class = Some("action_client_pkg::FibonacciClient".into());
        o.crate_name = Some("action_client_pkg".into());
        assert_eq!(
            component_type_path(&o).as_deref(),
            Some("action_client_pkg::FibonacciClient")
        );
        assert_eq!(crate_name(&o), Some("action_client_pkg"));
        let main = render_harness_main(&o).unwrap();
        assert!(main.contains("record_node_metadata::<action_client_pkg::FibonacciClient>"));
        let toml = render_harness_cargo_toml(&o).unwrap();
        assert!(toml.contains(
            "action_client_pkg = { path = \"/ws/src/demo_pkg\", package = \"action_client_pkg\" }"
        ));
    }

    /// A class without an explicit crate name still resolves: the class's own
    /// first segment IS the crate.
    #[test]
    fn crate_name_falls_back_to_the_class_head() {
        let mut o = opts();
        o.component_id = "talker".into();
        o.class = Some("talker_pkg::Talker".into());
        assert_eq!(crate_name(&o), Some("talker_pkg"));
    }

    #[test]
    fn harness_main_calls_record_and_serialize() {
        let main = render_harness_main(&opts()).unwrap();
        assert!(main.contains("record_node_metadata::<demo_pkg::talker::Component>"));
        assert!(main.contains("SourceMetadataExport::new(\"demo_pkg\", \"talker\")"));
        assert!(main.contains(".executable(\"talker\")"));
        assert!(main.contains(".exported_symbol(\"nros_component_talker\")"));
        assert!(main.contains("to_source_metadata_json"));
        assert!(main.contains("/out/talker.metadata.json"));
    }

    #[test]
    fn harness_cargo_toml_path_deps_nros_std_and_component() {
        let toml = render_harness_cargo_toml(&opts()).unwrap();
        assert!(
            toml.contains(
                "nros = { path = \"/nano-ros/packages/core/nros\", features = [\"std\"] }"
            )
        );
        assert!(
            toml.contains("demo_pkg = { path = \"/ws/src/demo_pkg\", package = \"demo_pkg\" }")
        );
    }

    /// A path dependency is looked up by PACKAGE name while the dep key must be
    /// the rustc-visible CRATE name — they differ for every hyphenated example
    /// (`qemu-rtic-action-client` → crate `qemu_rtic_action_client`), which used
    /// to fail "no matching package named `qemu_rtic_action_client` found".
    /// Cargo's `package = "…"` rename bridges the two.
    #[test]
    fn harness_cargo_toml_renames_hyphenated_component_packages() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"qemu-rtic-action-client\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let mut o = opts();
        o.component_dir = dir.path().to_path_buf();
        o.crate_name = Some("qemu_rtic_action_client".into());
        let toml = render_harness_cargo_toml(&o).unwrap();
        assert!(
            toml.contains("qemu_rtic_action_client = { path =")
                && toml.contains("package = \"qemu-rtic-action-client\""),
            "dep must be keyed by crate name and renamed to the package, got:\n{toml}"
        );
    }

    #[test]
    fn harness_main_omits_optional_export_fields_when_absent() {
        let mut o = opts();
        o.executable = None;
        o.exported_symbol = None;
        let main = render_harness_main(&o).unwrap();
        assert!(main.contains("SourceMetadataExport::new(\"demo_pkg\", \"talker\");"));
        assert!(!main.contains(".executable("));
        assert!(!main.contains(".exported_symbol("));
    }
}
