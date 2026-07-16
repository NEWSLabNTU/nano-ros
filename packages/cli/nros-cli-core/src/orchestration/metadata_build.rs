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
    /// Component id `crate::module` (e.g. `demo_pkg::talker`).
    pub component_id: String,
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

/// `crate::module` → `crate::module::Component` (the registered type). Mirrors
/// the generator's `rust_component_type_path`.
fn component_type_path(component_id: &str) -> Option<String> {
    let mut parts = component_id.split("::").filter(|p| !p.is_empty());
    let krate = parts.next()?;
    let module = parts.next()?;
    Some(format!("{krate}::{module}::Component"))
}

fn crate_name(component_id: &str) -> Option<&str> {
    component_id.split("::").next().filter(|s| !s.is_empty())
}

pub fn render_harness_cargo_toml(o: &MetadataBuildOptions) -> Result<String> {
    let krate = crate_name(&o.component_id)
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
         {krate} = {{ path = {comp:?} }}\n",
        nros = o
            .nano_ros_workspace
            .join("packages/core/nros")
            .display()
            .to_string(),
        comp = o.component_dir.display().to_string(),
    ))
}

pub fn render_harness_main(o: &MetadataBuildOptions) -> Result<String> {
    let type_path = component_type_path(&o.component_id)
        .ok_or_else(|| eyre!("component id '{}' is not `crate::module`", o.component_id))?;
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

    let manifest = o.harness_dir.join("Cargo.toml");
    let target_dir = o.harness_dir.join("target");
    let status = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
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
    fn type_path_and_crate_name() {
        assert_eq!(
            component_type_path("demo_pkg::talker").as_deref(),
            Some("demo_pkg::talker::Component")
        );
        assert_eq!(crate_name("demo_pkg::talker"), Some("demo_pkg"));
        assert_eq!(component_type_path("nocrate"), None); // needs crate::module
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
        assert!(toml.contains("demo_pkg = { path = \"/ws/src/demo_pkg\" }"));
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
