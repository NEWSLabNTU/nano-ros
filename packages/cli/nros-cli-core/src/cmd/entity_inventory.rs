//! phase-403 W9 (issue 0965) -- `nros ws entity-inventory`, the verb that turns
//! an image's component declarations into the three transports.
//!
//! Sibling of [`crate::cmd::entity_facts`], and deliberately shaped like it: one
//! resolution, one implementation. The DIFFERENCE is which question it answers
//! and where the answer comes from.
//!
//! `entity-facts` reads the resolved SystemModel and abstains on all 115 of
//! them, because a launch file names a node and never says what that node
//! wires. This verb reads `nros-metadata.json` -- the file
//! `nano_ros_node_register()` already writes, one row per component -- and the
//! `ENTITIES` the register call states. That is the one place in the build
//! where the wiring is both KNOWN and available before the sizes it feeds are
//! compiled; see [`crate::entity_inventory`] for why a link-section manifest
//! cannot be.
//!
//! Reading METADATA and not the C++ sources is the same decision
//! `codegen::entry::metadata` makes for `class` / `class_header`: the register
//! call is the declaration, and a second parser for the same fact is how the
//! two spellings drift.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, bail};
use serde::Deserialize;

use crate::entity_inventory::{
    ComponentEntities, Declaration, ENTITY_INVENTORY_CMAKE_NAME, ENTITY_INVENTORY_JSON_NAME,
    EntityDecl, EntityInventory,
};

/// The `components[]` fields this verb needs. Every other field the typed entry
/// emitter reads is ignored here, exactly as that emitter ignores these.
#[derive(Debug, Deserialize)]
struct ComponentMeta {
    name: String,
    #[serde(default)]
    pkg: Option<String>,
    class: String,
    /// phase-403 W9 -- `nano_ros_node_register(ENTITIES ...)`.
    ///
    /// `Option<Vec<_>>` and not `#[serde(default)]`: the whole design turns on
    /// telling "declared nothing" from "did not declare", and a defaulted empty
    /// vector collapses exactly those two.
    #[serde(default)]
    entities: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct MetadataDoc {
    #[serde(default)]
    components: Vec<ComponentMeta>,
}

#[derive(Debug, ClapArgs)]
pub struct EntityInventoryArgs {
    /// The `nros-metadata.json` an image's configure wrote. Defaults to
    /// `nros-metadata.json` in the current directory, which is where
    /// `_nros_metadata_emit()` puts it (`${CMAKE_BINARY_DIR}`).
    #[arg(long, value_name = "PATH")]
    pub metadata: Option<PathBuf>,

    /// Write the canonical JSON artifact here.
    #[arg(long = "output-json", value_name = "PATH")]
    pub output_json: Option<PathBuf>,

    /// Write the `include()`able CMake projection here.
    #[arg(long = "output-cmake", value_name = "PATH")]
    pub output_cmake: Option<PathBuf>,

    /// Write both artifacts into this directory, under their canonical names.
    #[arg(long = "output-dir", value_name = "DIR")]
    pub output_dir: Option<PathBuf>,

    /// Exit non-zero when the inventory REFUSES to derive.
    ///
    /// Off by default, and that is the load-bearing choice: a configure that has
    /// not yet registered every component is a normal intermediate state, the
    /// same one `nros_derive_message_bound_knobs` treats as "refused, every knob
    /// keeps its configured value". A build that WANTS the number to exist asks
    /// for it here.
    #[arg(long)]
    pub require_derived: bool,
}

/// Build the inventory from one parsed metadata document.
///
/// A pure function over the document, with file IO lifted out, for the reason
/// `entity_facts::facts_from_model` is: a sizing rule verified by reading is how
/// this campaign's other defects survived.
fn inventory_from_metadata(source: &str, doc: &MetadataDoc) -> Result<EntityInventory> {
    let mut inv = EntityInventory::new(source);
    for c in &doc.components {
        // Pre-RFC-0057 metadata carries no `pkg`; the retired L.4 convention
        // (`pkg = class.split("::").next()`) is the same fallback
        // `codegen::entry::metadata` keeps, restated rather than shared because
        // that module's index is keyed for a different consumer.
        let pkg = c
            .pkg
            .clone()
            .or_else(|| c.class.split("::").next().map(str::to_string))
            .unwrap_or_default();
        let declaration = match &c.entities {
            None => Declaration::Absent,
            Some(specs) => {
                let mut decls = Vec::new();
                let mut said_none = false;
                for spec in specs {
                    let spec = spec.trim();
                    if spec.is_empty() {
                        continue;
                    }
                    if spec.eq_ignore_ascii_case("none") {
                        said_none = true;
                        continue;
                    }
                    decls.extend(EntityDecl::parse(spec).map_err(|e| {
                        eyre::eyre!("component `{}::{}` declares `{spec}`: {e}", pkg, c.name)
                    })?);
                }
                if !decls.is_empty() && said_none {
                    bail!(
                        "component `{}::{}` declares both NONE and {} entities. \
                         NONE is an assertion that it creates none; the two cannot both hold.",
                        pkg,
                        c.name,
                        decls.len()
                    );
                }
                if decls.is_empty() && !said_none {
                    // An `ENTITIES` list that is present and empty says nothing,
                    // and "says nothing" must read as ABSENT so the refusal
                    // fires. It must NOT read as zero.
                    Declaration::Absent
                } else if said_none {
                    Declaration::None
                } else {
                    Declaration::Stated(decls)
                }
            }
        };
        inv.insert(ComponentEntities {
            pkg,
            component: c.name.clone(),
            class: c.class.clone(),
            declaration,
        });
    }
    Ok(inv)
}

pub fn run(args: EntityInventoryArgs) -> Result<()> {
    let metadata = args
        .metadata
        .clone()
        .unwrap_or_else(|| PathBuf::from("nros-metadata.json"));
    let raw = std::fs::read_to_string(&metadata)
        .wrap_err_with(|| format!("read metadata `{}`", metadata.display()))?;
    let doc: MetadataDoc = serde_json::from_str(&raw)
        .wrap_err_with(|| format!("parse metadata `{}`", metadata.display()))?;
    let inv = inventory_from_metadata(&metadata.display().to_string(), &doc)?;

    let (json_path, cmake_path) = match &args.output_dir {
        Some(dir) => (
            Some(
                args.output_json
                    .clone()
                    .unwrap_or(dir.join(ENTITY_INVENTORY_JSON_NAME)),
            ),
            Some(
                args.output_cmake
                    .clone()
                    .unwrap_or(dir.join(ENTITY_INVENTORY_CMAKE_NAME)),
            ),
        ),
        None => (args.output_json.clone(), args.output_cmake.clone()),
    };
    if let Some(p) = &json_path {
        write_if_changed(p, &inv.to_json())?;
    }
    if let Some(p) = &cmake_path {
        write_if_changed(p, &inv.to_cmake())?;
    }

    // The env transport goes to stdout, which is what makes this verb
    // interchangeable with `ws entity-facts` at a `corrosion_set_env_vars`
    // call site. Empty on a refusal, so nothing is exported and the reading
    // build script stays on its own default.
    print!("{}", inv.to_env());

    let derivation = inv.derive();
    if let crate::entity_inventory::Derivation::Refused { reason } = &derivation {
        if args.require_derived {
            bail!("entity inventory REFUSED to derive:\n  {reason}");
        }
        eprintln!("nros: entity inventory not derived -- {reason}");
    }
    Ok(())
}

/// The one write discipline (issues 0498/0562): atomic, and WRITE-IF-CHANGED.
///
/// Write-if-changed is load-bearing rather than tidy here: the CMake consumer
/// registers the fragment with `CMAKE_CONFIGURE_DEPENDS`, so rewriting it with
/// identical bytes on every configure would re-arm a re-configure forever.
/// Same reason `_nros_message_bounds_write_output` does it.
fn write_if_changed(path: &std::path::Path, content: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).wrap_err_with(|| format!("create `{}`", dir.display()))?;
        }
    }
    crate::atomic_file::atomic_write(path, content)
        .wrap_err_with(|| format!("write `{}`", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> EntityInventory {
        let doc: MetadataDoc = serde_json::from_str(raw).expect("metadata parses");
        inventory_from_metadata("test", &doc).expect("inventory builds")
    }

    /// The channel end to end: a register call's `ENTITIES` reaches the derived
    /// knob through `nros-metadata.json` and nothing else.
    #[test]
    fn entities_travel_from_the_metadata_row_to_the_knob() {
        let inv = parse(
            r#"{"components": [
                 {"name": "talker", "pkg": "demo", "class": "demo::Talker",
                  "entities": ["pub:std_msgs/msg/Int32:/chatter", "timer"]},
                 {"name": "listener", "pkg": "demo", "class": "demo::Listener",
                  "entities": ["sub:std_msgs/msg/Int32:/chatter"]}
               ]}"#,
        );
        let k = inv.derive().knobs().expect("derived").clone();
        assert_eq!(k.entity_total, 3);
        assert_eq!(k.max_cbs, 2, "the publisher claims no slot");
        // Issue 0900 — `NROS_EXECUTOR_ACTION_CLIENTS` rides the same carrier,
        // clamped by build.rs to the MAX_CBS emitted beside it.
        assert_eq!(
            inv.to_env(),
            "NROS_EXECUTOR_MAX_CBS=2\nNROS_EXECUTOR_ACTION_CLIENTS=0\n"
        );
    }

    /// A row with no `entities` KEY is the pre-W9 shape every existing
    /// component still has, and it must refuse rather than count as zero.
    #[test]
    fn a_row_with_no_entities_key_is_absent_not_zero() {
        let inv = parse(
            r#"{"components": [
                 {"name": "talker", "pkg": "demo", "class": "demo::Talker",
                  "entities": ["timer"]},
                 {"name": "legacy", "pkg": "demo", "class": "demo::Legacy"}
               ]}"#,
        );
        match inv.derive() {
            crate::entity_inventory::Derivation::Refused { reason } => {
                assert!(reason.contains("demo::legacy"), "{reason}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// An `ENTITIES` list that is present and EMPTY says nothing. It reads as
    /// absent, not as an assertion of zero -- `NONE` is that assertion.
    #[test]
    fn an_empty_entities_list_is_absent_and_none_is_an_answer() {
        let empty = parse(
            r#"{"components": [{"name": "n", "pkg": "p", "class": "p::N", "entities": []}]}"#,
        );
        assert!(matches!(
            empty.derive(),
            crate::entity_inventory::Derivation::Refused { .. }
        ));
        let none = parse(
            r#"{"components": [{"name": "n", "pkg": "p", "class": "p::N", "entities": ["none"]}]}"#,
        );
        assert_eq!(none.derive().knobs().expect("derived").max_cbs, 0);
    }

    /// NONE beside real entities is a contradiction, and it is an ERROR rather
    /// than a resolution in either direction. Picking one would make the other
    /// spelling silently wrong.
    #[test]
    fn none_beside_entities_is_rejected() {
        let doc: MetadataDoc = serde_json::from_str(
            r#"{"components": [
                 {"name": "n", "pkg": "p", "class": "p::N", "entities": ["none", "timer"]}]}"#,
        )
        .unwrap();
        let err = inventory_from_metadata("test", &doc)
            .unwrap_err()
            .to_string();
        assert!(err.contains("both NONE"), "{err}");
    }

    /// A bad spelling names the component, not just the token: metadata is
    /// machine-written and the user has to find the register call.
    #[test]
    fn a_bad_spelling_names_the_component() {
        let doc: MetadataDoc = serde_json::from_str(
            r#"{"components": [
                 {"name": "n", "pkg": "p", "class": "p::N", "entities": ["publsher"]}]}"#,
        )
        .unwrap();
        let err = inventory_from_metadata("test", &doc)
            .unwrap_err()
            .to_string();
        assert!(err.contains("p::n"), "{err}");
        assert!(err.contains("publsher"), "{err}");
    }

    /// Pre-RFC-0057 metadata has no `pkg`; the fallback keeps such a row
    /// identifiable in a refusal rather than dropping it.
    #[test]
    fn a_row_without_pkg_still_lands_in_the_inventory() {
        let inv = parse(r#"{"components": [{"name": "n", "class": "old::N"}]}"#);
        assert_eq!(inv.len(), 1);
        assert_eq!(inv.components()[0].pkg, "old");
    }
}
