//! `nros metadata` - collect generated component source metadata.

use crate::orchestration::{
    metadata_build::{MetadataBuildOptions, build_metadata},
    workspace::{ComponentDeclaration, Workspace},
};
use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, bail, eyre};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Mirrors `nros::MISSING_COMPONENT_EXPORT_ERROR` (in
/// `packages/core/nros/src/component.rs`) so host-side diagnostics
/// surface the same human-readable phrase as the in-tree
/// `ComponentError::MissingExport` runtime variant. Held as a
/// `const` here to keep the CLI off the `nros` build dependency
/// closure (the latter is `no_std` + target-feature-gated).
pub(crate) const MISSING_COMPONENT_EXPORT_ERROR: &str = "package has no exported nros component";

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// System package name used for build/<system_pkg>/nros output
    pub system_pkg: String,

    /// Workspace root containing colcon-like src/* packages
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// Output root for orchestration artifacts
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Existing source metadata JSON to validate and preserve
    #[arg(long = "metadata")]
    pub metadata: Vec<PathBuf>,

    /// Phase 172.E — produce missing source metadata by compiling + running
    /// each declared component in metadata mode (the metadata-mode build).
    #[arg(long)]
    pub build: bool,

    /// nano-ros workspace root for the metadata harness's `nros` dep
    /// (`--build`); falls back to the `NROS_WORKSPACE` env var.
    #[arg(long)]
    pub nano_ros_workspace: Option<PathBuf>,
}

/// Derive the metadata-mode build options for a declared component. The
/// component id is `crate::module` (the registered type is `::Component`);
/// the metadata header name is its last segment.
fn metadata_build_options(
    decl: &ComponentDeclaration,
    nano_ros: &Path,
    out_root: &Path,
) -> MetadataBuildOptions {
    let id = decl.config.component.clone();
    let name = id.rsplit("::").next().unwrap_or(&id).to_string();
    let probe = out_root.join("metadata-probe").join(id.replace("::", "__"));
    // W.3 (Phase 172): a minimal `[component]` may omit `[linkage]` — derive the
    // executable + exported symbol from the component name / crate convention.
    MetadataBuildOptions {
        component_id: id,
        package: decl.config.package.clone(),
        executable: Some(decl.config.linkage.resolved_executable(&name)),
        exported_symbol: Some(decl.config.linkage.resolved_exported_symbol(&name)),
        component: name,
        component_dir: decl.package_root.clone(),
        nano_ros_workspace: nano_ros.to_path_buf(),
        output_path: decl.source_metadata_path(),
        harness_dir: probe,
    }
}

pub fn run(args: Args) -> Result<()> {
    let root = args.workspace.unwrap_or(std::env::current_dir()?);
    let out_root = args
        .out_dir
        .unwrap_or_else(|| root.join("build").join(&args.system_pkg).join("nros"));
    let metadata_dir = out_root.join("metadata");
    fs::create_dir_all(&metadata_dir)?;

    let workspace = Workspace::discover(&root)?;

    // Phase 126.B.7 acceptance — every package that declared itself
    // a nros component (via `component_nros.toml`) must have actually
    // produced its source-metadata JSON. A missing file is the host-
    // side surface for "forgot to write `nros::component!`": the
    // metadata-mode binary either failed to build or built but exited
    // before writing the JSON, both shapes leave the declared
    // `[metadata].source_metadata` path empty. Catch the case here
    // with the same diagnostic string the in-tree
    // `ComponentError::MissingExport` runtime variant uses.
    let declarations = workspace.component_declarations()?;
    let mut missing: Vec<&ComponentDeclaration> = declarations
        .iter()
        .filter(|decl| !decl.source_metadata_path().is_file())
        .collect();

    // Phase 172.E — `--build` produces the missing source metadata by
    // compiling + running each declared component in metadata mode.
    let mut built: Vec<PathBuf> = Vec::new();
    if !missing.is_empty() && args.build {
        let nano_ros = args
            .nano_ros_workspace
            .clone()
            .or_else(|| std::env::var_os("NROS_WORKSPACE").map(PathBuf::from))
            .ok_or_else(|| {
                eyre!(
                    "`nros metadata --build` needs the nano-ros workspace — pass \
                     --nano-ros-workspace <path> or set NROS_WORKSPACE"
                )
            })?;
        for decl in &missing {
            let opts = metadata_build_options(decl, &nano_ros, &out_root);
            build_metadata(&opts)
                .wrap_err_with(|| format!("build source metadata for `{}`", decl.config.package))?;
            built.push(decl.source_metadata_path());
        }
        missing.clear();
    }

    if !missing.is_empty() {
        let mut msg = String::from(MISSING_COMPONENT_EXPORT_ERROR);
        for decl in &missing {
            msg.push_str(&format!(
                "\n  - package `{}`: expected source metadata at {}",
                decl.config.package,
                decl.source_metadata_path().display()
            ));
        }
        msg.push_str(
            "\n  hint: add `nros::component!(YourComponent);` to the package's `lib.rs`/`main.rs`, \
             then run `nros metadata --build` to produce it.",
        );
        bail!(msg);
    }

    // Collect: explicit `--metadata`, then anything `--build` just produced,
    // then (if neither) the workspace's discovered source-metadata files.
    let mut inputs = args.metadata;
    inputs.extend(built);
    if inputs.is_empty() {
        inputs.extend(workspace.source_metadata_files());
    }

    for path in &inputs {
        let raw = fs::read_to_string(path)
            .wrap_err_with(|| format!("failed to read source metadata {}", path.display()))?;
        let _: Value = serde_json::from_str(&raw)
            .wrap_err_with(|| format!("invalid source metadata JSON {}", path.display()))?;
        let file_name = path
            .file_name()
            .ok_or_else(|| eyre!("metadata path has no file name: {}", path.display()))?;
        fs::write(metadata_dir.join(file_name), raw)?;
    }

    eprintln!(
        "nros metadata: preserved {} metadata artifact(s) in {}",
        inputs.len(),
        metadata_dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::{
        config::{ComponentConfig, ComponentLinkage, ComponentMetadataConfig, ComponentOverrides},
        source_metadata::ComponentLanguage,
        workspace::ComponentDeclaration,
    };

    #[test]
    fn metadata_build_options_derives_id_name_and_paths() {
        let decl = ComponentDeclaration {
            package_root: PathBuf::from("/ws/src/demo_pkg"),
            manifest_path: PathBuf::from("/ws/src/demo_pkg/component_nros.toml"),
            config: ComponentConfig {
                version: 1,
                package: "demo_pkg".into(),
                component: "demo_pkg::talker".into(),
                language: ComponentLanguage::Rust,
                linkage: ComponentLinkage {
                    crate_name: Some("demo_pkg".into()),
                    executable: Some("talker".into()),
                    exported_symbol: Some("nros_component_talker".into()),
                    static_library: None,
                },
                metadata: ComponentMetadataConfig {
                    source_metadata: "talker.metadata.json".into(),
                    generated_by: None,
                },
                overrides: ComponentOverrides {
                    default_namespace: None,
                    parameters: Default::default(),
                    remaps: Vec::new(),
                },
            },
        };
        let o = metadata_build_options(&decl, Path::new("/nano-ros"), Path::new("/out"));
        assert_eq!(o.component_id, "demo_pkg::talker");
        assert_eq!(o.component, "talker"); // last segment of the id
        assert_eq!(o.package, "demo_pkg");
        assert_eq!(o.executable.as_deref(), Some("talker"));
        assert_eq!(o.exported_symbol.as_deref(), Some("nros_component_talker"));
        assert_eq!(o.component_dir, PathBuf::from("/ws/src/demo_pkg"));
        assert_eq!(o.nano_ros_workspace, PathBuf::from("/nano-ros"));
        // source_metadata resolves relative to the package root.
        assert_eq!(
            o.output_path,
            PathBuf::from("/ws/src/demo_pkg/talker.metadata.json")
        );
        // probe dir is per-component under the out root.
        assert_eq!(
            o.harness_dir,
            PathBuf::from("/out/metadata-probe/demo_pkg__talker")
        );
    }

    /// W.3 (Phase 172): a `[component]` with no `[linkage]` still yields a usable
    /// executable + exported symbol, derived from the component's short name.
    #[test]
    fn metadata_build_options_derives_linkage_when_absent() {
        let decl = ComponentDeclaration {
            package_root: PathBuf::from("/ws/src/demo_pkg"),
            manifest_path: PathBuf::from("/ws/src/demo_pkg/nros.toml"),
            config: ComponentConfig {
                version: 1,
                package: "demo_pkg".into(),
                component: "demo_pkg::talker".into(),
                language: ComponentLanguage::Rust,
                linkage: ComponentLinkage::default(), // no [linkage] table
                metadata: ComponentMetadataConfig {
                    source_metadata: "talker.metadata.json".into(),
                    generated_by: None,
                },
                overrides: ComponentOverrides::default(),
            },
        };
        let o = metadata_build_options(&decl, Path::new("/nano-ros"), Path::new("/out"));
        assert_eq!(o.executable.as_deref(), Some("talker"));
        assert_eq!(o.exported_symbol.as_deref(), Some("nros_component_talker"));
    }
}
