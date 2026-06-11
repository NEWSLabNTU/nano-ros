//! Phase 212.B.4 — `[package.metadata.ament]` reader entry point.
//!
//! The strict schema lives at [`cargo_metadata_schema::PackageMetadataAment`];
//! this module is the path-based entry point that callers (`nros emit
//! package-xml`'s `render_for_pkg`, `nros check`, and the workspace
//! loader's per-pkg cargo metadata sweep) reach for when they have a pkg
//! dir on disk and want the parsed table without re-implementing the
//! Cargo.toml read-and-extract.
//!
//! Usage:
//!
//! ```ignore
//! use std::path::Path;
//! use nros_cli_core::orchestration::ament::parse_ament_metadata;
//!
//! let ament = parse_ament_metadata(Path::new("./talker_pkg"))?;
//! println!("license = {:?}", ament.license);
//! ```
//!
//! Behaviour:
//!
//! * `<pkg-dir>/Cargo.toml` absent → returns `PackageMetadataAment::default()`
//!   (no ament block authored).
//! * Present but no `[package.metadata.ament]` → same.
//! * Present and authored → strict-mode parse; serde rejects unknown
//!   fields with the underlying diagnostic verbatim.

use std::path::Path;

use eyre::{Context, Result, eyre};
use serde::Deserialize;

use super::cargo_metadata_schema::PackageMetadataAment;

/// Re-export so callsites can write `ament::AmentMetadata` if they prefer
/// the role-named alias to the schema struct name.
pub type AmentMetadata = PackageMetadataAment;

/// Read `<pkg-dir>/Cargo.toml` and return its `[package.metadata.ament]`
/// table parsed against the strict schema. Returns
/// `PackageMetadataAment::default()` when the manifest exists but the
/// table is absent — matching the policy of every other Phase 212
/// per-pkg reader (silent default is fine; only typos surface as
/// hard errors).
pub fn parse_ament_metadata(pkg_dir: &Path) -> Result<AmentMetadata> {
    let cargo_toml = pkg_dir.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return Ok(AmentMetadata::default());
    }
    let raw = std::fs::read_to_string(&cargo_toml)
        .with_context(|| format!("read {}", cargo_toml.display()))?;
    let manifest: AmentBearingManifest =
        toml::from_str(&raw).with_context(|| format!("parse {}", cargo_toml.display()))?;
    let ament = manifest
        .package
        .and_then(|p| p.metadata)
        .and_then(|m| m.ament)
        .unwrap_or_default();
    Ok(ament)
}

/// Minimal `Cargo.toml` projection that exposes only the ament sub-table.
/// We avoid `#[serde(deny_unknown_fields)]` here on the outer structs
/// because a real Cargo manifest carries dozens of fields we don't model
/// — the strictness lives on [`PackageMetadataAment`] itself.
#[derive(Debug, Deserialize)]
struct AmentBearingManifest {
    #[serde(default)]
    package: Option<AmentBearingPackage>,
}

#[derive(Debug, Deserialize)]
struct AmentBearingPackage {
    #[serde(default)]
    metadata: Option<AmentBearingPackageMetadata>,
}

#[derive(Debug, Deserialize)]
struct AmentBearingPackageMetadata {
    #[serde(default)]
    ament: Option<PackageMetadataAment>,
}

/// Convenience: parse a raw `[package.metadata.ament]` table out of an
/// already-deserialised `serde_json::Value` (the shape `cargo metadata`
/// hands back). Mirrors the in-loader fn in
/// [`super::nros_config`] so external callers can reuse the same parse
/// path without going through cargo metadata themselves.
///
/// Returns `Ok(PackageMetadataAment::default())` when the input value has
/// no `ament` key.
pub fn parse_ament_metadata_value(value: &serde_json::Value) -> Result<AmentMetadata> {
    let Some(ament) = value.get("ament") else {
        return Ok(AmentMetadata::default());
    };
    PackageMetadataAment::deserialize(ament.clone())
        .map_err(|e| eyre!("invalid [package.metadata.ament]: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn scratch_pkg(name: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "nros-ament-{name}-{}-{stamp}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn parse_ament_metadata_basic_round_trip() {
        let p = scratch_pkg("basic");
        fs::write(
            p.join("Cargo.toml"),
            r#"
[package]
name = "talker_pkg"
version = "0.1.0"
edition = "2021"

[package.metadata.ament]
description = "Talker."
license = "Apache-2.0"
maintainer = { name = "Ada Lovelace", email = "ada@example.com" }
buildtool_depend = ["ament_cargo"]
exec_depend = ["std_msgs", "rcl_interfaces"]
build_depend = ["std_msgs"]
"#,
        )
        .unwrap();
        let ament = parse_ament_metadata(&p).expect("parse");
        assert_eq!(ament.description.as_deref(), Some("Talker."));
        assert_eq!(ament.license.as_deref(), Some("Apache-2.0"));
        let m = ament.maintainer.as_ref().expect("maintainer");
        assert_eq!(m.name, "Ada Lovelace");
        assert_eq!(m.email, "ada@example.com");
        assert_eq!(ament.buildtool_depend, vec!["ament_cargo"]);
        assert_eq!(ament.exec_depend, vec!["std_msgs", "rcl_interfaces"]);
        assert_eq!(ament.build_depend, vec!["std_msgs"]);
    }

    #[test]
    fn parse_ament_metadata_returns_default_when_no_cargo_toml() {
        let p = scratch_pkg("nocargo");
        // No Cargo.toml at all.
        let ament = parse_ament_metadata(&p).expect("parse");
        assert_eq!(ament, AmentMetadata::default());
    }

    #[test]
    fn parse_ament_metadata_returns_default_when_table_absent() {
        let p = scratch_pkg("notable");
        fs::write(
            p.join("Cargo.toml"),
            r#"
[package]
name = "no_ament_pkg"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        let ament = parse_ament_metadata(&p).expect("parse");
        assert_eq!(ament, AmentMetadata::default());
    }

    #[test]
    fn parse_ament_metadata_rejects_typo() {
        let p = scratch_pkg("typo");
        fs::write(
            p.join("Cargo.toml"),
            r#"
[package]
name = "p"
version = "0.1.0"
edition = "2021"

[package.metadata.ament]
descripshun = "typo here"
"#,
        )
        .unwrap();
        let err = parse_ament_metadata(&p).expect_err("typo must reject");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("descripshun") || msg.contains("unknown field"),
            "diagnostic: {msg}"
        );
    }
}
