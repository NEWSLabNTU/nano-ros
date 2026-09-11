//! phase-403 W9 (issue 0965) -- `nros ws entity-inventory`, the verb that turns
//! an image's component declarations into the three transports.
//!
//! Sibling of [`crate::cmd::entity_facts`], and deliberately shaped like it: one
//! resolution, one implementation. The DIFFERENCE is which question it answers
//! and where the answer comes from.
//!
//! `entity-facts` reads the resolved SystemModel and ABSTAINS unless somebody
//! authored a `<stem>.contract.yaml` beside the launch file, because a launch
//! file names a node and never says what that node wires (issue 0973 —
//! measured 2026-09-06: 5 of the tree's 114 resolvable models describe wiring,
//! and they are exactly the 5 with a contract). This verb reads
//! `nros-metadata.json` -- the file `nano_ros_node_register()` already writes,
//! one row per component -- and, with `--model`, the same contract through the
//! resolved model. That is the one place in the build where the wiring is both
//! KNOWN and available before the sizes it feeds are compiled; see
//! [`crate::entity_inventory`] for why a link-section manifest cannot be.
//!
//! The `ENTITIES` argument that used to carry the per-component half is
//! RETIRED (phase-412): it was hand-maintained beside the code with nothing
//! comparing the two, and on the safety island it drifted. The contract file is
//! now the single statement of what an image creates.
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

    /// phase-412 -- a resolved SystemModel whose `structure.topics` carries the
    /// authored contract's wiring.
    ///
    /// When given AND the model describes wiring, its per-node sub/pub sets are
    /// combined with the metadata declaration per component and per kind,
    /// taking whichever says more (`EntityInventory::merged_per_kind_max`).
    /// That is what lets a contract beside the launch file replace the
    /// `ENTITIES` lists, WITHOUT losing the timers the contract schema cannot
    /// express.
    ///
    /// Absent, or present but describing no wiring, the metadata declaration
    /// stands alone -- which is every image in this tree that has not authored
    /// a contract.
    #[arg(long, value_name = "PATH")]
    pub model: Option<PathBuf>,

    /// Write the canonical JSON artifact here.
    #[arg(long = "output-json", value_name = "PATH")]
    pub output_json: Option<PathBuf>,

    /// Write the `include()`able CMake projection here.
    #[arg(long = "output-cmake", value_name = "PATH")]
    pub output_cmake: Option<PathBuf>,

    /// Write both artifacts into this directory, under their canonical names.
    #[arg(long = "output-dir", value_name = "DIR")]
    pub output_dir: Option<PathBuf>,

    /// Write the C++ compile-time depth table here (phase-403 step 2).
    ///
    /// `nros/declared_qos.hpp` expands it and `NROS_SUBSCRIBE` static_asserts
    /// against it, so this is the transport that carries a declared `@depth=`
    /// all the way to the compiler.
    #[arg(long = "output-header", value_name = "PATH")]
    pub output_header: Option<PathBuf>,

    /// Write the C++ table of each node's DECLARED parameters here (phase-446
    /// W6), read from `--model`'s `contracts.node_params`.
    ///
    /// `nros/declared_params.hpp` expands it and
    /// `Node::declare_parameter` refuses a name the node's contract
    /// does not declare, or a type that differs. Written even with no model or
    /// no declaration -- then it defines no table and nothing is checked -- so
    /// a stale table from an earlier configure can never outlive its contract.
    #[arg(long = "output-params-header", value_name = "PATH")]
    pub output_params_header: Option<PathBuf>,

    /// Restrict the inventory to ONE component, by `<pkg>::<name>` or `<name>`.
    ///
    /// For `--output-header` from inside `nano_ros_node_register()`, which runs
    /// once per component and knows only its own declaration. The metadata file
    /// at that point holds every component registered SO FAR, and a table built
    /// over that accidental prefix would differ between a clean configure and a
    /// warm one. Naming the component makes the output a function of the
    /// declaration rather than of the configure order.
    ///
    /// A name that matches nothing is an ERROR, not an empty table: an empty
    /// table is what a silently-misspelled component would look like, and it
    /// disables every check for that component's TU.
    #[arg(long = "component", value_name = "PKG::NAME")]
    pub component: Option<String>,

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
    let mut inv = inventory_from_metadata(&metadata.display().to_string(), &doc)?;

    // phase-412 -- fold in the model's wiring when a contract authored it.
    //
    // Deliberately a COMBINE and not a replace: the contract has no timer
    // entity, so a model-only inventory under-sizes MAX_CBS by one per timer
    // in the image. See `EntityInventory::merged_per_kind_max`.
    // phase-446 W6 -- rendered from the same model, when there is one.
    let mut params_header: Option<String> = None;
    if let Some(model_path) = &args.model {
        let raw = std::fs::read_to_string(model_path)
            .wrap_err_with(|| format!("read model `{}`", model_path.display()))?;
        let model: ros_launch_manifest_model::SystemModel = serde_yaml_ng::from_str(&raw)
            .wrap_err_with(|| format!("parse model `{}`", model_path.display()))?;
        reject_zero_depths(&model).wrap_err_with(|| format!("model `{}`", model_path.display()))?;
        // phase-454 W3 -- and a QoS VALUE this build does not model. Same place
        // and same reason as the zero depth above: this is the one point a model
        // enters the verb and the only one with an error channel.
        reject_unknown_qos_values(&model)
            .wrap_err_with(|| format!("model `{}`", model_path.display()))?;
        if args.output_params_header.is_some() {
            params_header = Some(crate::declared_params_header::render(
                Some(&model),
                &model_path.display().to_string(),
            ));
        }
        // `from_model` returning None means NO WIRING DESCRIBED. Not an error
        // and not a zero: nobody authored a contract for this image, so the
        // declaration is the only source there is and it stands alone.
        if let Some(model_inv) =
            EntityInventory::from_model(model_path.display().to_string(), &model)
        {
            inv = inv.merged_per_kind_max(&model_inv);
        }
        // phase-446 W4 -- the contract's `params:` size the parameter store.
        // Attached whether or not the model describes wiring: the two answers
        // are independent, and a model with no topics can still declare
        // parameters.
        inv.set_param_declarations(crate::entity_inventory::ParamDeclarations::from_model(
            &model,
        ));
    }

    if let Some(want) = &args.component {
        inv = narrow_to_component(&inv, want)?;
    }

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
    if let Some(p) = &args.output_header {
        write_if_changed(p, &inv.to_declared_qos_header())?;
    }
    if let Some(p) = &args.output_params_header {
        let h = params_header
            .unwrap_or_else(|| crate::declared_params_header::render(None, "no --model given"));
        write_if_changed(p, &h)?;
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

/// A contract may not state `depth: 0` (issue 1084).
///
/// The `ENTITIES` grammar refused `@depth=0` outright -- `KEEP_LAST(0)` holds
/// no sample, so a zero is a typo for "I did not want to say", and the two
/// license opposite actions: a stated depth SIZES the arena and asserts at
/// every call site, an absent one makes a size consumer REFUSE. When the
/// contract replaced `ENTITIES` as the producer that rule did not travel with
/// it: `qos: { depth: 0 }` parsed, reached [`EntityDecl::depth`] as `Some(0)`
/// and would have rendered a table row that fails the build at every
/// `NROS_SUBSCRIBE` on that topic, naming a number no author meant.
///
/// Refused here, at the one place a model enters this verb, rather than in
/// `from_model` -- which returns `Option` to say "no wiring described" and has
/// no channel for "what you wrote is wrong".
///
/// phase-454 W2 -- BOTH endpoint maps. The rule is a property of `KEEP_LAST(0)`
/// and not of which side of a topic states it, and the `ENTITIES` grammar it
/// inherits refuses `@depth=0` on every kind that carries a depth at all
/// ([`EntityKind::carries_qos_depth`], publishers included). Reading only
/// `sub_endpoints` was correct for exactly as long as a publisher's depth could
/// not travel; now that it does, a `pub: { qos: { depth: 0 } }` would reach
/// `EntityDecl::depth` as `Some(0)` and render a row stating a number no author
/// meant -- the same defect issue 1084 fixed one map over.
fn reject_zero_depths(model: &ros_launch_manifest_model::SystemModel) -> Result<()> {
    let subs = model
        .contracts
        .sub_endpoints
        .iter()
        .map(|(ep, c)| ("subscriber", ep, c.qos.as_ref().and_then(|q| q.depth)));
    let pubs = model
        .contracts
        .pub_endpoints
        .iter()
        .map(|(ep, c)| ("publisher", ep, c.qos.as_ref().and_then(|q| q.depth)));
    for (side, ep, depth) in subs.chain(pubs) {
        if depth == Some(0) {
            bail!(
                "contract {side} endpoint `{ep}` states `qos: {{ depth: 0 }}`. A QoS depth \
                 of 0 states nothing -- KEEP_LAST(0) holds no sample. Omit `depth:` to say \
                 \"not declared\", which is a different claim and the one that makes a size \
                 consumer REFUSE rather than guess."
            );
        }
    }
    Ok(())
}

/// A contract may not state a QoS value this build does not model
/// (phase-454 W3, issue 1256).
///
/// The issue's own acceptance says it: *"an unknown value is an error, never a
/// skip."* `Qos::{reliability, durability, history}` are free-form `String`s in
/// the model -- the resolver carries whatever the contract wrote -- so
/// `reliability: best-effort` (a hyphen) or `reliability: BestEffort` parses,
/// resolves, and would reach [`EntityInventory::from_model`]'s
/// `parse_reliability` as `None`, which is the spelling for NOBODY SAID. A
/// misspelling would then be indistinguishable from silence: the endpoint would
/// be counted as undeclared, every consumer would refuse on the safe side, and
/// the author would never learn that the line they wrote does nothing.
///
/// Refused HERE and not in `from_model`, for exactly the reason
/// [`reject_zero_depths`] is: that function returns `Option` to say "no wiring
/// described" and has no channel for "what you wrote is wrong".
///
/// The accepted spellings come from `nros_orchestration_ir::qos_override`, the
/// module that already owned this vocabulary for `qos_overrides.*` parameters.
/// One vocabulary, two surfaces -- which is also what will let phase-454 W7
/// compare the two statements for agreement (RFC-0100 D8) rather than compare
/// two spellings of two parsers.
/// `pub(crate)` because the verb is not the only road a model reaches
/// `from_model` on: `nros build`'s stage-3.5 seed (`cmd::build`) composes the
/// same inventory from the same model, and a check that guards one of two roads
/// is the shape issue 1199 names. That road records the refusal rather than
/// failing the process, because a seed that cannot answer is a normal state --
/// the configure-time producer is where the same value becomes fatal.
pub(crate) fn reject_unknown_qos_values(
    model: &ros_launch_manifest_model::SystemModel,
) -> Result<()> {
    use nros_orchestration_ir::qos_override as qos;

    let subs = model
        .contracts
        .sub_endpoints
        .iter()
        .map(|(ep, c)| ("subscriber", ep, c.qos.as_ref()));
    let pubs = model
        .contracts
        .pub_endpoints
        .iter()
        .map(|(ep, c)| ("publisher", ep, c.qos.as_ref()));
    for (side, ep, q) in subs.chain(pubs) {
        let Some(q) = q else { continue };
        // `(policy name, what the contract wrote, did it parse, accepted)`.
        // Written as a table so a fourth policy is a row, and so that no policy
        // can be checked in one place and forgotten in the other -- the
        // `filter_map`-away shape `qos_override`'s own header warns about.
        let checks: [(&str, Option<&str>, bool, &str); 3] = [
            (
                "reliability",
                q.reliability.as_deref(),
                q.reliability
                    .as_deref()
                    .is_none_or(|v| qos::parse_reliability(v).is_some()),
                qos::RELIABILITY_VALUES,
            ),
            (
                "durability",
                q.durability.as_deref(),
                q.durability
                    .as_deref()
                    .is_none_or(|v| qos::parse_durability(v).is_some()),
                qos::DURABILITY_VALUES,
            ),
            (
                "history",
                q.history.as_deref(),
                q.history
                    .as_deref()
                    .is_none_or(|v| qos::parse_history(v).is_some()),
                qos::HISTORY_VALUES,
            ),
        ];
        for (policy, written, ok, accepted) in checks {
            if ok {
                continue;
            }
            bail!(
                "contract {side} endpoint `{ep}` states `qos: {{ {policy}: {} }}`, which is not \
                 a {policy} this build models (expected {accepted}). It is REFUSED rather than \
                 ignored: an unreadable value would be counted as \"not declared\", every size \
                 consumer would refuse on the safe side, and you would never learn that the \
                 line does nothing.",
                written.unwrap_or("")
            );
        }
    }
    Ok(())
}

/// Keep only the named component (phase-403 step 2).
///
/// Matches `<pkg>::<component>` first, then a bare `<component>`, and a bare
/// name that matches more than one component is an ERROR rather than a pick:
/// two packages may each register a `talker`, and silently choosing one would
/// give a TU a table describing a different node.
fn narrow_to_component(inv: &EntityInventory, want: &str) -> Result<EntityInventory> {
    let want = want.trim();
    let hits: Vec<&crate::entity_inventory::ComponentEntities> = inv
        .components()
        .into_iter()
        .filter(|c| format!("{}::{}", c.pkg, c.component) == want || c.component == want)
        .collect();
    match hits.len() {
        1 => {
            let mut narrowed = EntityInventory::new(format!("{} [{}]", inv.source, want));
            narrowed.insert(hits[0].clone());
            Ok(narrowed)
        }
        0 => bail!(
            "no component named `{want}` in this metadata. It holds: {}",
            inv.components()
                .iter()
                .map(|c| format!("{}::{}", c.pkg, c.component))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        n => {
            bail!("`{want}` names {n} components in this metadata; qualify it as `<pkg>::{want}`.")
        }
    }
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
            "NROS_EXECUTOR_MAX_CBS=2\nNROS_EXECUTOR_ACTION_CLIENTS=0\n\
             NROS_RUNTIME_MAX_CELL_ENTITIES=1\n"
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

    // -----------------------------------------------------------------
    // phase-403 step 2.
    // -----------------------------------------------------------------

    /// `--component` exists so `nano_ros_node_register()` can render a header
    /// for the component it is registering, from a metadata file that holds
    /// every component registered SO FAR. Without the narrowing the table would
    /// be a function of the configure ORDER rather than of the declaration.
    #[test]
    fn narrowing_to_a_component_keeps_only_that_row() {
        let inv = parse(
            r#"{"components": [
                 {"name": "talker", "pkg": "demo", "class": "demo::Talker",
                  "entities": ["pub:std_msgs/msg/Int32:/chatter@depth=1"]},
                 {"name": "listener", "pkg": "demo", "class": "demo::Listener",
                  "entities": ["sub:std_msgs/msg/Int32:/chatter@depth=7"]}
               ]}"#,
        );
        let one = narrow_to_component(&inv, "demo::listener").expect("narrows");
        assert_eq!(one.len(), 1);
        assert!(one.to_declared_qos_header().contains("\"/chatter\", 7"));
        // The bare name works too, and a name that matches NOTHING is an error
        // rather than an empty table: an empty table disables every check for
        // that TU, which is exactly what a misspelling would produce.
        assert!(narrow_to_component(&inv, "listener").is_ok());
        let err = narrow_to_component(&inv, "lisener")
            .unwrap_err()
            .to_string();
        assert!(err.contains("demo::listener"), "names what IS there: {err}");
    }

    /// A bare name that matches two components is an ERROR, not a pick. Two
    /// packages may each register a `talker`, and silently choosing one gives a
    /// TU a table describing a different node -- a check that passes while
    /// asserting against the wrong declaration.
    #[test]
    fn an_ambiguous_bare_component_name_is_rejected() {
        let inv = parse(
            r#"{"components": [
                 {"name": "talker", "pkg": "a", "class": "a::T", "entities": ["timer"]},
                 {"name": "talker", "pkg": "b", "class": "b::T", "entities": ["timer"]}
               ]}"#,
        );
        let err = narrow_to_component(&inv, "talker").unwrap_err().to_string();
        assert!(err.contains("2 components"), "{err}");
        assert!(err.contains("<pkg>::talker"), "names the fix: {err}");
    }

    /// issue 1084 -- a contract may not spell "not declared" as `depth: 0`.
    ///
    /// The same rule `@depth=0` has always had, applied at the producer that
    /// replaced `ENTITIES`. A zero would otherwise render a table row and fail
    /// the build at every call site on that topic, naming a number nobody
    /// meant; and it must not be silently DROPPED either, because a dropped
    /// declaration is one the author believes they made.
    #[test]
    fn a_contract_depth_of_zero_is_rejected_rather_than_rendered_or_dropped() {
        let model: ros_launch_manifest_model::SystemModel = serde_yaml_ng::from_str(
            r#"
meta: { version: 1 }
structure:
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      sub: [/listener/chatter]
contracts:
  sub_endpoints:
    /listener/chatter:
      qos: { depth: 0 }
"#,
        )
        .expect("model fixture parses");
        let err = reject_zero_depths(&model).unwrap_err().to_string();
        assert!(
            err.contains("/listener/chatter"),
            "names the endpoint: {err}"
        );
        assert!(err.contains("states nothing"), "{err}");
        assert!(err.contains("REFUSE"), "names what absence buys: {err}");
    }

    /// phase-454 W2 -- and the same rule on the PUBLISHER side, now that a
    /// publisher's depth travels.
    ///
    /// `KEEP_LAST(0)` holds no sample whichever side of the topic states it,
    /// and the `ENTITIES` grammar this inherits refuses `@depth=0` on every
    /// kind that carries a depth. Reading only `sub_endpoints` was correct for
    /// exactly as long as `from_model` dropped publisher depths.
    #[test]
    fn a_publisher_contract_depth_of_zero_is_rejected_too() {
        let model: ros_launch_manifest_model::SystemModel = serde_yaml_ng::from_str(
            r#"
meta: { version: 1 }
structure:
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
contracts:
  pub_endpoints:
    /talker/chatter:
      qos: { depth: 0 }
"#,
        )
        .expect("model fixture parses");
        let err = reject_zero_depths(&model).unwrap_err().to_string();
        assert!(err.contains("/talker/chatter"), "names the endpoint: {err}");
        assert!(
            err.contains("publisher"),
            "names which side stated it: {err}"
        );
        assert!(err.contains("states nothing"), "{err}");
    }

    /// ...and a contract that states a real depth, or none at all, passes.
    #[test]
    fn a_contract_without_a_zero_depth_is_accepted() {
        let model: ros_launch_manifest_model::SystemModel = serde_yaml_ng::from_str(
            r#"
meta: { version: 1 }
structure:
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      sub: [/listener/chatter, /other/chatter]
contracts:
  sub_endpoints:
    /listener/chatter:
      qos: { depth: 1 }
    /other/chatter:
      max_age_ms: 100.0
"#,
        )
        .expect("model fixture parses");
        reject_zero_depths(&model).expect("a stated depth and a silent endpoint are both legal");
    }

    /// phase-454 W3 (issue 1256) -- an unknown QoS VALUE is an error, never a
    /// skip.
    ///
    /// The issue's acceptance in one sentence, and the reason it has to be an
    /// error: `reliability:` is a free-form `String` in the model, so a
    /// misspelling parses to `None` inside `from_model`, which is the spelling
    /// for NOBODY SAID. Silence and a typo would then be the same fact -- every
    /// size consumer would refuse on the safe side, the build would succeed,
    /// and the author would never learn that the line does nothing.
    ///
    /// All three policies, because a check written once per policy is a check
    /// that gets written for two of them (`qos_override`'s own header: both
    /// producers `filter_map`ed an unrecognised policy away, and the copies
    /// disagreed).
    #[test]
    fn an_unknown_qos_spelling_is_rejected_before_it_can_be_dropped() {
        // (the line to write, the fragment the message must name, the accepted
        // spellings it must offer)
        let cases: &[(&str, &str, &str)] = &[
            (
                "reliability: best-effort",
                "best-effort",
                "`best_effort` or `reliable`",
            ),
            (
                "durability: TransientLocal",
                "TransientLocal",
                "`volatile` or `transient_local`",
            ),
            ("history: keep-all", "keep-all", "`keep_last` or `keep_all`"),
        ];
        for (line, written, accepted) in cases {
            let model: ros_launch_manifest_model::SystemModel = serde_yaml_ng::from_str(&format!(
                r#"
meta: {{ version: 1 }}
structure:
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      sub: [/listener/chatter]
contracts:
  sub_endpoints:
    /listener/chatter:
      qos: {{ {line} }}
"#
            ))
            .expect("model fixture parses");
            let err = reject_unknown_qos_values(&model).unwrap_err().to_string();
            assert!(
                err.contains("/listener/chatter"),
                "names the endpoint: {err}"
            );
            assert!(err.contains(written), "quotes what was written: {err}");
            assert!(err.contains(accepted), "names what is accepted: {err}");
            assert!(
                err.contains("not declared"),
                "and says what the silent drop would have looked like: {err}"
            );
        }
    }

    /// ...on the PUBLISHER side too. W2 made a publisher's QoS travel; a check
    /// reading only `sub_endpoints` is issue 1084's defect one map over.
    #[test]
    fn an_unknown_qos_spelling_on_a_publisher_is_rejected_too() {
        let model: ros_launch_manifest_model::SystemModel = serde_yaml_ng::from_str(
            r#"
meta: { version: 1 }
structure:
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
contracts:
  pub_endpoints:
    /talker/chatter:
      qos: { durability: transient-local }
"#,
        )
        .expect("model fixture parses");
        let err = reject_unknown_qos_values(&model).unwrap_err().to_string();
        assert!(err.contains("/talker/chatter"), "{err}");
        assert!(err.contains("publisher"), "names which side: {err}");
    }

    /// ...and the VERB reaches that check, not just the function.
    ///
    /// The two tests above call `reject_unknown_qos_values` directly, which
    /// proves the rule and says nothing about whether anything runs it -- the
    /// exact shape issue 1226 names ("a gate that WORKS is not a gate that
    /// RUNS"). This one drives [`run`] end to end over a real metadata file and
    /// a real model, so deleting the call site is a red rather than a silent
    /// return to the drop this wave fixed.
    #[test]
    fn the_verb_itself_refuses_an_unreadable_qos_value() {
        let dir = scratch_dir("qos-verb");
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let metadata = dir.join("nros-metadata.json");
        std::fs::write(
            &metadata,
            r#"{"components": [
                 {"name": "listener", "pkg": "demo", "class": "demo::Listener",
                  "entities": ["sub:std_msgs/msg/Int32:/chatter"]}
               ]}"#,
        )
        .expect("write metadata");
        let model = dir.join("system_model.yaml");
        std::fs::write(
            &model,
            r#"
meta: { version: 1 }
structure:
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      sub: [/listener/chatter]
contracts:
  sub_endpoints:
    /listener/chatter:
      qos: { reliability: best-effort }
"#,
        )
        .expect("write model");

        let args = |model: Option<&std::path::Path>| EntityInventoryArgs {
            metadata: Some(metadata.clone()),
            model: model.map(|p| p.to_path_buf()),
            output_json: None,
            output_cmake: None,
            output_dir: None,
            output_header: None,
            output_params_header: None,
            component: None,
            require_derived: false,
        };
        // The whole CHAIN: the verb wraps the check's message in "model
        // `<path>`", so the outermost `to_string()` alone would pass on any
        // model-shaped failure at all.
        let err =
            run(args(Some(&model))).expect_err("the verb must refuse a QoS value it cannot read");
        let chain: String = err
            .chain()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            chain.contains("best-effort"),
            "the verb's error must carry the check's message: {chain}"
        );
        assert!(chain.contains("/listener/chatter"), "{chain}");

        // The control: the SAME image with no model at all is fine, so the
        // failure above is the check firing and not the fixture being broken.
        run(args(None)).expect("a metadata-only run has no contract to check");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Unique scratch dir under the repo's gitignored `tmp/` (repo rule: temp
    /// files live in `$project/tmp/`, not the system temp dir).
    fn scratch_dir(name: &str) -> PathBuf {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        repo.join("tmp")
            .join(format!("{name}-{}-{stamp}", std::process::id()))
    }

    /// ...and every spelling this build DOES model passes, including the one
    /// that will make a size consumer refuse.
    ///
    /// `keep_all` is legal to WRITE. It refuses the depth-derived facts
    /// (RFC-0100 D6) and it is not a contract error, which is the distinction
    /// this test pins: a build that rejected it outright would be telling
    /// authors they may not ask for an unbounded queue.
    #[test]
    fn every_modelled_qos_spelling_is_accepted_keep_all_included() {
        let model: ros_launch_manifest_model::SystemModel = serde_yaml_ng::from_str(
            r#"
meta: { version: 1 }
structure:
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
      sub: [/listener/chatter]
contracts:
  pub_endpoints:
    /talker/chatter:
      qos: { reliability: reliable, durability: transient_local, history: keep_last }
  sub_endpoints:
    /listener/chatter:
      qos: { reliability: best_effort, durability: volatile, history: keep_all }
"#,
        )
        .expect("model fixture parses");
        reject_unknown_qos_values(&model).expect("every spelling here is one this build models");
    }

    /// The committed C++ compile fixture is exactly what this emitter renders.
    ///
    /// `packages/api/nros-cpp/tests/compile/declared-qos-fixture/` holds a
    /// generated header that `just check cpp` compiles against -- the table the
    /// positive TU asserts on and the negative TU is rejected by. A checked-in
    /// artifact with no gate is a copy that drifts, and this one drifts
    /// SILENTLY in the worst direction: a table whose keys stop matching leaves
    /// every `static_assert` in that gate vacuously true, and the gate green.
    ///
    /// Fix a failure by regenerating, never by hand-editing the header:
    ///
    ///   nros ws entity-inventory \
    ///     --metadata packages/api/nros-cpp/tests/compile/declared-qos-fixture/entities.json \
    ///     --component demo::listener \
    ///     --output-header packages/api/nros-cpp/tests/compile/declared-qos-fixture/nros/nros_declared_qos_generated.h
    #[test]
    fn the_committed_compile_fixture_is_what_this_emitter_renders() {
        const REL: &str = "packages/api/nros-cpp/tests/compile/declared-qos-fixture";
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("repo root");
        let dir = root.join(REL);
        let input = std::fs::read_to_string(dir.join("entities.json"))
            .unwrap_or_else(|e| panic!("read {}/entities.json: {e}", dir.display()));
        let doc: MetadataDoc = serde_json::from_str(&input).expect("fixture metadata parses");
        // The SOURCE line is part of the rendered file, and the CLI puts the
        // `--metadata` argument there verbatim -- so the fixture is generated
        // from the repo-relative path and regenerated the same way.
        let inv = inventory_from_metadata(&format!("{REL}/entities.json"), &doc)
            .expect("fixture inventory builds");
        let narrowed = narrow_to_component(&inv, "demo::listener").expect("narrows");
        let want = std::fs::read_to_string(dir.join("nros/nros_declared_qos_generated.h"))
            .expect("the committed fixture header exists");
        assert_eq!(
            narrowed.to_declared_qos_header(),
            want,
            "the committed declared-QoS fixture header is not what this emitter renders. \
             Regenerate it (see this test's doc comment) rather than editing it: a stale \
             fixture leaves `just check cpp`'s declared-depth assertions green and vacuous."
        );
    }
}
