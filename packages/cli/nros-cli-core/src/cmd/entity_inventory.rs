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
    ComponentEntities, Declaration, DeclaredQosHeaderTable, ENTITY_INVENTORY_CMAKE_NAME,
    ENTITY_INVENTORY_JSON_NAME, EntityInventory,
};

/// The `components[]` fields this verb needs. Every other field the typed entry
/// emitter reads is ignored here, exactly as that emitter ignores these.
#[derive(Debug, Deserialize)]
struct ComponentMeta {
    name: String,
    #[serde(default)]
    pkg: Option<String>,
    class: String,
    /// RETIRED (issue 1555) -- a metadata row says WHICH components an image
    /// registers, never WHAT they create.
    ///
    /// This used to be read. `nano_ros_node_register(ENTITIES ...)` wrote it
    /// until phase-412 retired the keyword, and phase-454 W9 removed the splice
    /// point from `_nros_metadata_emit()`. From then on the key had NO
    /// production producer -- measured 2026-09-29 over 248 real
    /// `nros-metadata.json` build artifacts, 0 carried it -- and the reader
    /// survived only because the declared-QoS compile fixture was a metadata
    /// document with the key in it. That fixture now states its depths the way
    /// a real image does, in a contract resolved into a model on `--model`
    /// (`packages/api/nros-cpp/tests/compile/declared-qos-fixture/declared_qos.yaml`),
    /// so the reader went.
    ///
    /// Deserialized as `IgnoredAny` and REFUSED rather than dropped from the
    /// struct: serde ignores an unknown key, so a document still carrying one
    /// would otherwise have its declaration discarded in silence -- a
    /// declaration its author believes they made. `check-knob-single-reader.py`
    /// keeps a reader from coming back.
    #[serde(default)]
    entities: Option<serde::de::IgnoredAny>,
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
    ///
    /// REPEATABLE, for the per-component compile-time tables only (issue
    /// 1564). A configure that declares several entries resolves several
    /// models and compiles each component ONCE for every image that launches
    /// it, so `--output-header` / `--output-params-header` then render the
    /// UNION of one table per model -- refusing, naming both models, where two
    /// of them state different values for one endpoint. Every IMAGE-wide
    /// output (`--output-json`, `--output-cmake`, `--output-dir`, the env on
    /// stdout) describes ONE image, so with more than one model those are
    /// refused rather than computed from a merge nobody defined.
    #[arg(long, value_name = "PATH")]
    pub model: Vec<PathBuf>,

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
        if c.entities.is_some() {
            bail!(
                "component `{pkg}::{}` in `{source}` carries an `entities` key. That key \
                 is RETIRED (issue 1555): no build writes it, and what a component creates \
                 is stated in the contract sidecar beside the launch file, which reaches \
                 this verb through `--model`. A standalone leaf states it in its \
                 `system.toml` `[[component]]` row instead.",
                c.name
            );
        }
        // The metadata names the REGISTERED population and nothing else, so
        // every row starts ABSENT: "did not declare" is the one true answer a
        // registration can give about what the component creates. `--model`
        // is what turns a row into a statement (`merged_per_kind_max`).
        let declaration = Declaration::Absent;
        inv.insert(ComponentEntities {
            pkg,
            component: c.name.clone(),
            class: c.class.clone(),
            declaration,
        });
    }
    Ok(inv)
}

/// Read one `nros-metadata.json` off disk and build its inventory.
///
/// phase-457 W0 (issue 1407) — the SECOND caller is
/// [`crate::cmd::sizing_descriptor`]'s model-road producer, which must derive
/// over the same component population this verb does. Lifted to a function
/// rather than copied there: a second read of one artifact is how two producers
/// of one schema come to describe different images (issue 1228's shape), and
/// the IO half is exactly what [`inventory_from_metadata`] was written without.
pub fn inventory_from_metadata_file(path: &std::path::Path) -> Result<EntityInventory> {
    let raw = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("read metadata `{}`", path.display()))?;
    let doc: MetadataDoc = serde_json::from_str(&raw)
        .wrap_err_with(|| format!("parse metadata `{}`", path.display()))?;
    inventory_from_metadata(&path.display().to_string(), &doc)
}

/// One `--model`, verified, parsed and checked at the door -- the three
/// refusals every model meets before it may say anything.
fn load_model(model_path: &std::path::Path) -> Result<ros_launch_manifest_model::SystemModel> {
    // phase-460 W1 (issue 1420) -- verify at this door. cmake hands over a
    // path `model-path` already verified; a hand run meets the same gate.
    // The bringup is recovered from where `nros sync` put the model.
    crate::model_gate::verify(model_path, None)
        .map_err(|e| eyre::eyre!("entity-inventory: {e}"))?;
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
    // phase-454 W7 (RFC-0100 D8) -- and a `qos_overrides.*` parameter that
    // states a capacity policy the contract does not. Third refusal at the
    // same seam, for the same reason: this is the one point a model enters
    // the verb and the only one with an error channel.
    reject_qos_override_divergence(&model)
        .wrap_err_with(|| format!("model `{}`", model_path.display()))?;
    Ok(model)
}

/// The metadata inventory with one model's wiring folded in, plus whether the
/// model described any wiring at all.
///
/// Deliberately a COMBINE and not a replace: the contract has no timer entity,
/// so a model-only inventory under-sizes MAX_CBS by one per timer in the
/// image. See `EntityInventory::merged_per_kind_max`.
fn with_model(
    metadata_inv: &EntityInventory,
    model_source: &str,
    model: &ros_launch_manifest_model::SystemModel,
) -> (EntityInventory, bool) {
    let mut inv = metadata_inv.clone();
    // `from_model` returning None means NO WIRING DESCRIBED. Not an error and
    // not a zero: nobody authored a contract for this image, so the
    // declaration is the only source there is and it stands alone.
    let wired = match EntityInventory::from_model(model_source, model) {
        Some(model_inv) => {
            inv = inv.merged_per_kind_max(&model_inv);
            true
        }
        None => false,
    };
    // phase-446 W4 -- the contract's `params:` size the parameter store.
    // Attached whether or not the model describes wiring: the two answers are
    // independent, and a model with no topics can still declare parameters.
    inv.set_param_declarations(crate::entity_inventory::ParamDeclarations::from_model(
        model,
    ));
    (inv, wired)
}

/// `--model` given more than once -- issue 1564.
///
/// A configure that declares several entries resolves several SystemModels and
/// compiles each COMPONENT once for all of them. Until this existed the
/// per-component render ABSTAINED in that case, and that is not a corner: the
/// native road builds every entry of a workspace in one configure, so
/// `examples/workspaces/cpp` (seven models) compiled all six of its components
/// with every declared-QoS and declared-parameter check off -- which is why no
/// C++ image in the tree could adopt the check at all.
///
/// Only the two per-component outputs are defined here. They are a UNION: see
/// [`DeclaredQosHeaderTable::union`] and `declared_params_header::render_union`
/// for the rule and why a conflict refuses. Everything else this verb writes
/// describes ONE image, and a "merge" of several images' entity counts is a
/// number nobody defined -- so those are refused, never computed.
fn run_several_models(args: &EntityInventoryArgs, metadata_inv: &EntityInventory) -> Result<()> {
    if args.output_json.is_some() || args.output_cmake.is_some() || args.output_dir.is_some() {
        eyre::bail!(
            "entity-inventory: --model was given {} times. Only the per-component tables \
             (--output-header, --output-params-header) are defined over several models; the \
             JSON and CMake projections describe ONE image and must be rendered per model.",
            args.model.len()
        );
    }
    if args.component.is_none() {
        eyre::bail!(
            "entity-inventory: --model was given {} times without --component. The union of \
             several models' tables is defined per COMPONENT -- the unit compiled once for all \
             of them.",
            args.model.len()
        );
    }
    let mut models: Vec<(String, ros_launch_manifest_model::SystemModel)> = Vec::new();
    for p in &args.model {
        models.push((p.display().to_string(), load_model(p)?));
    }

    if let Some(out) = &args.output_header {
        let mut tables: Vec<(String, DeclaredQosHeaderTable)> = Vec::new();
        let mut first_unwired: Option<(String, DeclaredQosHeaderTable)> = None;
        for (src, model) in &models {
            let (inv, wired) = with_model(metadata_inv, src, model);
            let inv = narrow_to_component(&inv, args.component.as_deref().expect("checked above"))?;
            let table = inv.declared_qos_header_table();
            if wired {
                tables.push((src.clone(), table));
            } else if first_unwired.is_none() {
                // A model that describes no wiring declares NOTHING -- it has
                // no contract. That is not a refusal to merge, it is silence,
                // and silence cannot contradict another model's declaration.
                first_unwired = Some((src.clone(), table));
            }
        }
        let (source, table) = match tables.len() {
            // No model describes wiring: render exactly what a lone such model
            // renders, so "nobody declared" reads the same with one entry or
            // with seven.
            0 => first_unwired.expect("at least two models were loaded"),
            _ => {
                let source = tables
                    .iter()
                    .map(|(s, _)| s.as_str())
                    .collect::<Vec<_>>()
                    .join(" + ");
                (source, DeclaredQosHeaderTable::union(&tables))
            }
        };
        if let DeclaredQosHeaderTable::Refused { reason } = &table {
            if tables.len() > 1 {
                // Loud, because the old abstention was loud, and because a
                // conflict between two contracts is something the author can
                // fix. The header says the same thing in a comment.
                eprintln!(
                    "nros: declared QoS: no table for {} -- {}",
                    args.component.as_deref().unwrap_or("?"),
                    reason.lines().next().unwrap_or("")
                );
            }
        }
        write_if_changed(
            out,
            &crate::entity_inventory::render_declared_qos_header(&source, &table),
        )?;
    }
    if let Some(out) = &args.output_params_header {
        let refs: Vec<(String, &ros_launch_manifest_model::SystemModel)> =
            models.iter().map(|(s, m)| (s.clone(), m)).collect();
        write_if_changed(out, &crate::declared_params_header::render_union(&refs))?;
    }
    Ok(())
}

pub fn run(args: EntityInventoryArgs) -> Result<()> {
    let metadata = args
        .metadata
        .clone()
        .unwrap_or_else(|| PathBuf::from("nros-metadata.json"));
    let metadata_inv = inventory_from_metadata_file(&metadata)?;

    if args.model.len() > 1 {
        return run_several_models(&args, &metadata_inv);
    }

    // phase-412 -- fold in the model's wiring when a contract authored it.
    //
    // Deliberately a COMBINE and not a replace, though no longer for the
    // reason this comment used to give ("the contract has no timer entity" --
    // a timer path in `contracts.node_paths` is one, and the metadata's entity
    // terms are always ABSENT since issue 1555 retired that reader). What the
    // metadata still contributes is the REGISTERED component population, which
    // the node table needs and the contract names only for the nodes somebody
    // wrote one for (issue 1407). See `EntityInventory::merged_per_kind_max`.
    // phase-446 W6 -- rendered from the same model, when there is one.
    let mut inv = metadata_inv;
    let mut params_header: Option<String> = None;
    if let Some(model_path) = args.model.first() {
        let model = load_model(model_path)?;
        if args.output_params_header.is_some() {
            params_header = Some(crate::declared_params_header::render(
                Some(&model),
                &model_path.display().to_string(),
            ));
        }
        inv = fold_model(inv, &model_path.display().to_string(), &model);
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

    // phase-454 W8 (RFC-0100 D9) -- the two `buffer:` diagnostics.
    //
    // WARNINGS, on stderr, and deliberately not gated behind `--require-derived`
    // or anything else: both shapes are legal, so there is no flag under which
    // they should become fatal. Stderr and not stdout because stdout is this
    // verb's ENV TRANSPORT -- a line printed there lands in a
    // `corrosion_set_env_vars` call site and would be parsed as a variable.
    for d in inv.buffer_diagnostics() {
        eprintln!("nros: warning: {}", d.message());
    }

    // ...and the visible REASON an endpoint that asked for the derivation did
    // not get one (RFC-0100 D9, acceptance 3).
    //
    // Narrowed to endpoints that declared `buffer: queue`, which is the whole
    // population that asked. A line per subscription would be noise on every
    // build; a silent decline for an author who wrote `buffer: queue` and got
    // no default is the failure this campaign keeps paying for -- a derivation
    // that declines quietly is indistinguishable from one that never ran.
    for row in inv.queue_depth_defaults() {
        if row.buffer != Some(crate::queue_depth::BufferDiscipline::Queue) {
            continue;
        }
        if let Err(reason) = &row.outcome {
            // A stated depth is not a failure -- it is the rung above working.
            if matches!(reason, crate::queue_depth::NoDefault::DepthStated(_)) {
                continue;
            }
            eprintln!("nros: {}", row.line());
        }
    }

    let derivation = inv.derive();
    if let crate::entity_inventory::Derivation::Refused { reason } = &derivation {
        if args.require_derived {
            bail!("entity inventory REFUSED to derive:\n  {reason}");
        }
        eprintln!("nros: entity inventory not derived -- {reason}");
    }
    Ok(())
}

/// Fold a resolved model into the metadata's inventory -- the whole
/// composition [`run`] performs once the model has passed its refusals.
///
/// Lifted out of `run` so the committed declared-QoS compile fixture is held to
/// THIS function rather than to a restatement of it
/// (`the_committed_compile_fixture_is_what_this_emitter_renders`): a fixture
/// rendered by a second spelling of the composition is how a gate comes to
/// assert something no configure produces.
fn fold_model(
    inv: EntityInventory,
    model_source: &str,
    model: &ros_launch_manifest_model::SystemModel,
) -> EntityInventory {
    // One fold, two callers: the single-model `run` and the fixture test want
    // only the inventory; the multi-model union also needs to know whether the
    // model described any wiring, which is `with_model`'s second value.
    with_model(&inv, model_source, model).0
}

/// A contract may not state `depth: 0` (issue 1084).
///
/// The `ENTITIES` grammar refused `@depth=0` outright -- `KEEP_LAST(0)` holds
/// no sample, so a zero is a typo for "I did not want to say", and the two
/// license opposite actions: a stated depth SIZES the arena and asserts at
/// every call site, an absent one makes a size consumer REFUSE. When the
/// contract replaced `ENTITIES` as the producer that rule did not travel with
/// it: `qos: { depth: 0 }` parsed, reached [`crate::entity_inventory::EntityDecl::depth`] as `Some(0)`
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

/// The contract and `qos_overrides.*` must state the same QoS (phase-454 W7,
/// RFC-0100 D8).
///
/// `qos_overrides.<topic>.<role>.<policy>` feeds the baked RUNTIME table and the
/// contract feeds SIZING, and until W7 the two never met:
/// `qos_overrides./t.subscription.depth = 64` reached the runtime and the arena
/// never heard about it -- issue 1190's `BufferTooSmall` with a config-shaped
/// cause and no diagnostic.
///
/// A thin wrapper over [`nros_orchestration_ir::qos_agreement::check_model`],
/// which is where the rule lives because the `nros::main!` proc-macro cannot dep
/// this crate and it must ask the same question.
///
/// FOUR call sites, not two. `reject_unknown_qos_values` guards the two SIZING
/// roads (this verb and `nros build`'s seed) because those are the two roads a
/// model reaches `from_model` on. This rule also needs the two BAKE roads
/// (`codegen::entry::plan_from_model` and the proc-macro), because only a bake
/// is guaranteed to see an override: an image can bake
/// `qos_overrides./t.subscription.depth = 64` without ever running this verb
/// with a `--model`. A check that guards a subset of the roads a fact travels
/// is the shape issue 1199 names.
pub(crate) fn reject_qos_override_divergence(
    model: &ros_launch_manifest_model::SystemModel,
) -> Result<()> {
    nros_orchestration_ir::qos_agreement::check_model(model).map_err(|e| eyre::eyre!("{e}"))
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

    /// The metadata half composed with a model, through the same fold `run`
    /// performs.
    fn compose(meta: &str, model_yaml: &str) -> EntityInventory {
        let model: ros_launch_manifest_model::SystemModel =
            serde_yaml_ng::from_str(model_yaml).expect("model fixture parses");
        fold_model(parse(meta), "model", &model)
    }

    /// Issue 1555 -- the metadata says WHICH components an image registers and
    /// never WHAT they create, so with no model every row is ABSENT and the
    /// derivation REFUSES rather than reading the image as empty.
    #[test]
    fn a_metadata_row_alone_is_absent_never_zero() {
        let inv = parse(
            r#"{"components": [{"name": "talker", "pkg": "demo", "class": "demo::Talker"}]}"#,
        );
        match inv.derive() {
            crate::entity_inventory::Derivation::Refused { reason } => {
                assert!(reason.contains("demo::talker"), "{reason}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// Issue 1555 -- the retired `entities` key is REFUSED, never ignored. serde
    /// drops an unknown key in silence, so a document that still carried one
    /// would lose its declaration without a word; the refusal names the issue
    /// and where the statement lives now.
    #[test]
    fn the_retired_entities_key_is_refused_not_ignored() {
        let doc: MetadataDoc = serde_json::from_str(
            r#"{"components": [
                 {"name": "n", "pkg": "p", "class": "p::N", "entities": ["timer"]}]}"#,
        )
        .unwrap();
        let err = inventory_from_metadata("test", &doc)
            .unwrap_err()
            .to_string();
        assert!(err.contains("p::n"), "names the component: {err}");
        assert!(err.contains("1555"), "names the issue: {err}");
        assert!(err.contains("--model"), "names where it lives now: {err}");
    }

    /// The channel end to end on the road a real image takes: the metadata's
    /// registered rows, the contract's wiring through the model, one knob.
    #[test]
    fn the_model_states_what_the_registered_components_create() {
        let inv = compose(
            r#"{"components": [
                 {"name": "talker", "pkg": "demo", "class": "demo::Talker"},
                 {"name": "listener", "pkg": "demo", "class": "demo::Listener"}
               ]}"#,
            r#"
meta: { version: 1 }
structure:
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
      sub: [/listener/chatter]
contracts:
  node_paths:
    /talker/on_tick:
      output: [/talker/chatter]
"#,
        );
        let k = inv.derive().knobs().expect("derived").clone();
        assert_eq!(k.entity_total, 3);
        assert_eq!(k.max_cbs, 2, "the publisher claims no slot");
        assert_eq!(
            inv.to_env(),
            "NROS_EXECUTOR_MAX_CBS=2\nNROS_EXECUTOR_ACTION_CLIENTS=0\n\
             NROS_RUNTIME_MAX_CELL_ENTITIES=1\n"
        );
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
        let inv = compose(
            r#"{"components": [
                 {"name": "talker", "pkg": "demo", "class": "demo::Talker"},
                 {"name": "listener", "pkg": "demo", "class": "demo::Listener"}
               ]}"#,
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
      qos: { depth: 1 }
  sub_endpoints:
    /listener/chatter:
      qos: { depth: 7 }
"#,
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
                 {"name": "talker", "pkg": "a", "class": "a::T"},
                 {"name": "talker", "pkg": "b", "class": "b::T"}
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
                 {"name": "listener", "pkg": "demo", "class": "demo::Listener"}
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
            model: model.map(|p| p.to_path_buf()).into_iter().collect(),
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
    /// generated header that `just check c` and `just check cpp` compile
    /// against -- the table the positive TU asserts on and the negative TU is
    /// rejected by. A checked-in artifact with no gate is a copy that drifts,
    /// and this one drifts SILENTLY in the worst direction: a table whose keys
    /// stop matching leaves every `static_assert` in that gate vacuously true,
    /// and the gate green.
    ///
    /// Issue 1555 -- its INPUT is the one a real image's header comes from: the
    /// `nros-metadata.json` a configure writes (which components are
    /// registered) folded with a resolved model whose contract states the
    /// depths, through [`fold_model`] -- the function `run` calls. It used to be
    /// a metadata document carrying an `"entities"` key, which nothing in a
    /// production build writes.
    ///
    /// Fix a failure by regenerating, never by hand-editing the header:
    ///
    ///   nros ws entity-inventory \
    ///     --metadata packages/api/nros-cpp/tests/compile/declared-qos-fixture/nros-metadata.json \
    ///     --model packages/api/nros-cpp/tests/compile/declared-qos-fixture/declared_qos.yaml \
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
        let meta_raw = std::fs::read_to_string(dir.join("nros-metadata.json"))
            .unwrap_or_else(|e| panic!("read {}/nros-metadata.json: {e}", dir.display()));
        let doc: MetadataDoc = serde_json::from_str(&meta_raw).expect("fixture metadata parses");
        let model_raw = std::fs::read_to_string(dir.join("declared_qos.yaml"))
            .unwrap_or_else(|e| panic!("read {}/declared_qos.yaml: {e}", dir.display()));
        let model: ros_launch_manifest_model::SystemModel =
            serde_yaml_ng::from_str(&model_raw).expect("fixture model parses");
        // The refusals `run` applies at the same door, so a fixture that a
        // configure would reject cannot be rendered here either.
        reject_zero_depths(&model).expect("fixture states no zero depth");
        reject_unknown_qos_values(&model).expect("fixture states only modelled QoS");
        reject_qos_override_divergence(&model).expect("fixture has no divergent override");
        // The SOURCE line is part of the rendered file, and the CLI puts the
        // `--metadata` / `--model` arguments there verbatim -- so the fixture is
        // generated from the repo-relative paths and regenerated the same way.
        let inv = inventory_from_metadata(&format!("{REL}/nros-metadata.json"), &doc)
            .expect("fixture inventory builds");
        let inv = fold_model(inv, &format!("{REL}/declared_qos.yaml"), &model);
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
