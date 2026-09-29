//! phase-454 W4 — `nros ws sizing-descriptor`, the READ side of RFC-0100 D4.
//!
//! `nros sync` writes `build/nros/sizing/<entry>.toml`. This verb reads one back
//! and answers the two questions a consumer that is not a Rust build script has:
//!
//! * **cmake** — `--output-cmake <path>` writes an `include()`able projection.
//!   CMake does not parse TOML and must not learn to: the schema has ONE reader
//!   ([`nros_sizing_descriptor`]) and a second parser in a `.cmake` file is the
//!   drift class the FFI-mirror gates police one layer down. The cmake road
//!   reaches the same reader through this verb.
//! * **a human** — with no `--output-cmake`, the summary on stdout, one line per
//!   fact, refusals included. `[meta] status` says whether declaring more would
//!   buy anything; the per-field statuses say what.
//!
//! The freshness rule the cmake side owes is issue 1018's: `execute_process()`
//! has already run by the time ninja decides anything, so a configure-time
//! reader must register BOTH the descriptor and this tool in
//! `CMAKE_CONFIGURE_DEPENDS`. `nros_sizing_descriptor_read()` in
//! `cmake/NanoRosSizingDescriptor.cmake` does both; this verb is only the answer.
//!
//! # `--from-model` — the WRITE side, for a road that is not cargo (phase-454 W14)
//!
//! `nros sync` writes a descriptor for a single-package cargo LEAF, where
//! `cmd::leaf_settings::write` has the leaf's inventories to hand. A cmake /
//! Zephyr west / NuttX entry has no such moment: its one statement of what the
//! image declares is the resolved SystemModel, and the only process that reads
//! it during a configure is this CLI. So the write side lives here, beside the
//! read side, rather than as a second producer nobody can reach from cmake.
//!
//! **A model that describes no wiring writes NOTHING and says so.**
//! `EntityInventory::from_model` returns `None` there, and an all-refused file
//! would put an artifact in front of seven consumers in order to say nothing —
//! while moving the `[meta] basis` every one of them guards on. "No contract, no
//! change" is W12's own control and it holds on this road too.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr};

#[derive(Debug, ClapArgs)]
pub struct SizingDescriptorArgs {
    /// The descriptor to READ. `<build>/nros/sizing/<entry>.toml`.
    #[arg(long, value_name = "PATH", conflicts_with = "from_model")]
    pub descriptor: Option<PathBuf>,

    /// Write the `include()`able CMake projection here.
    #[arg(long, value_name = "PATH")]
    pub output_cmake: Option<PathBuf>,

    /// phase-454 W14 — WRITE a descriptor from this resolved SystemModel.
    ///
    /// The model-only road: counts and all four QoS policies are stated, and
    /// every field that needs a leaf's inventories is refused by name.
    #[arg(long, value_name = "PATH", requires = "build_dir", requires = "entry")]
    pub from_model: Option<PathBuf>,

    /// phase-457 W0.b (issues 1407 / 1378) — WRITE a descriptor for a STANDALONE
    /// LEAF, from its own `system.toml` `[[component]] entities`.
    ///
    /// The road `nros ws entity-facts --leaf` already answers and the one issue
    /// 1378 measured FAILING: a copy-out cmake project has no bringup and no
    /// SystemModel, so `--from-model` can never reach it, and until this mode
    /// existed the four `NROS_DECLARED_*` queryable facts were the only thing
    /// that described such an image. Same `EntityDecl` grammar, same counting
    /// rules, same refusals as the model road — only the input differs.
    #[arg(
        long,
        value_name = "DIR",
        conflicts_with = "descriptor",
        conflicts_with = "from_model",
        requires = "build_dir",
        requires = "entry"
    )]
    pub from_leaf: Option<PathBuf>,

    /// `--from-model` / `--from-leaf`: the image's build dir. The descriptor
    /// lands at `<build-dir>/nros/sizing/<entry>.toml`.
    #[arg(long, value_name = "DIR")]
    pub build_dir: Option<PathBuf>,

    /// `--from-model`: the entry name, which becomes `[meta] entry`.
    #[arg(long, value_name = "NAME")]
    pub entry: Option<String>,

    /// phase-457 W0 (issue 1407) — the `nros-metadata.json` this configure
    /// wrote, composed with `--from-model`'s contract exactly as `nros ws
    /// entity-inventory` composes it.
    ///
    /// The two producers of one descriptor schema must read ONE inventory. The
    /// model names the nodes a CONTRACT describes; the metadata names every
    /// component `nano_ros_node_register()` put in the image, which is the
    /// population a node table has to hold. Deriving over the contract's set
    /// alone publishes a count smaller than the image needs — see
    /// [`crate::entity_inventory::EntityInventory::merged_per_kind_max`] for
    /// why this is a per-kind MAX and not a replacement.
    #[arg(long, value_name = "PATH")]
    pub metadata: Option<PathBuf>,

    /// phase-457-payload W2 — a bound table this image's interface closure
    /// REGISTERED: the `nros_message_bounds.json` codegen emitted beside a
    /// `nros_message_bounds.cmake` fragment. Repeat once per table.
    ///
    /// `--from-model` / `--from-leaf` only. Read through the same reader the
    /// cargo leaf road uses over its `generated/` tree, so a cmake image and a
    /// cargo leaf with one contract price one type one way. None given keeps
    /// the refusal naming issue 1393; a table named here and not on disk yet is
    /// a per-field refusal naming its package, never an error — on the
    /// non-Zephyr cmake lane that is the ordinary first-configure state.
    #[arg(long = "bound-inventory", value_name = "PATH")]
    pub bound_inventory: Vec<PathBuf>,

    /// `--from-model`: the backend this image links, when it names one.
    #[arg(long, value_name = "NAME")]
    pub rmw: Option<String>,

    /// `--from-model`: the rustc triple the board pins, when the caller knows it.
    #[arg(long, value_name = "TRIPLE")]
    pub target_triple: Option<String>,

    /// `--from-model`: the target IS this host, so `[target]` may be read here.
    ///
    /// The one case RFC-0100 D1's "build scripts run for the host" does not
    /// bite: a native configure's target is the machine running this process.
    #[arg(long)]
    pub host_build: bool,

    /// `--from-model`: `[board.knobs.memory] heap_bytes`, when the caller knows it.
    #[arg(long, value_name = "BYTES")]
    pub heap_budget_bytes: Option<usize>,

    /// The road, in prose, for every refusal written.
    ///
    /// phase-457 W0.b — `Option`, with the default resolved PER MODE
    /// ([`DEFAULT_MODEL_ROAD`] / [`DEFAULT_LEAF_ROAD`]) rather than by clap. A
    /// single `default_value` is a claim about which producer is running, and it
    /// was wrong for the one added later: `--from-leaf` with no `--road` wrote
    /// "a cmake entry" into every refusal on a road that has no entry at all.
    /// A refusal's job is to name what is missing and where, so prose that names
    /// the wrong road is the defect this whole schema exists to prevent.
    #[arg(long, value_name = "PROSE")]
    pub road: Option<String>,
}

/// `--road`'s default for `--from-model`.
///
/// The cmake entry road is the one that reaches this producer in a build
/// (`nros_sizing_descriptor_from_model` passes it explicitly); a workspace cargo
/// image passes its own. Named so the two defaults sit beside each other and a
/// reader can see that they differ on purpose.
pub const DEFAULT_MODEL_ROAD: &str = "a cmake entry";

/// `--road`'s default for `--from-leaf`.
pub const DEFAULT_LEAF_ROAD: &str = "a standalone cmake leaf";

pub fn run(args: SizingDescriptorArgs) -> Result<()> {
    if let Some(model) = &args.from_model {
        return write_from_model(&args, model);
    }
    if let Some(leaf) = &args.from_leaf {
        return write_from_leaf(&args, leaf);
    }
    let Some(descriptor) = &args.descriptor else {
        eyre::bail!(
            "pass `--descriptor <path>` to read one, or `--from-model <path>` / \
             `--from-leaf <dir>` to write one"
        );
    };
    // A MISSING descriptor is an error here and not a shrug. The caller named a
    // path; "it wasn't there so I printed nothing" is the silent-default shape
    // RFC-0100 D6 exists to forbid, and cmake would `include()` an empty file
    // and size from its own literals believing it had been told.
    let desc =
        nros_sizing_descriptor::read(descriptor).wrap_err("reading the sizing descriptor")?;

    if let Some(out) = &args.output_cmake {
        let body = crate::sizing_descriptor::to_cmake(&desc);
        if let Some(dir) = out.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).wrap_err_with(|| format!("create `{}`", dir.display()))?;
        }
        // Write-if-changed: the consumer registers the result with
        // `CMAKE_CONFIGURE_DEPENDS`, so identical bytes must keep their mtime or
        // every configure re-arms the next one (issue 1018's sibling failure).
        crate::atomic_file::atomic_write(out, &body)
            .map_err(|e| eyre::eyre!("write `{}`: {e}", out.display()))?;
        return Ok(());
    }

    print!("{}", summary(&desc));
    Ok(())
}

/// phase-454 W14 — the model-only WRITE.
///
/// Prints the path it wrote on stdout, so a caller that did not want to spell
/// the path rule a second time can read it back. A model with no wiring prints
/// nothing and exits 0: the absence of a contract is a normal state, not a
/// broken configure, and it is the state 109 of 114 resolvable models are in.
fn write_from_model(args: &SizingDescriptorArgs, model_path: &std::path::Path) -> Result<()> {
    let (Some(build_dir), Some(entry)) = (&args.build_dir, &args.entry) else {
        // `requires =` above already enforces this; the bail is the one that
        // survives a caller reaching `run` without clap (a test, a future verb).
        eyre::bail!("`--from-model` needs `--build-dir` and `--entry`");
    };

    let raw = std::fs::read_to_string(model_path)
        .wrap_err_with(|| format!("reading `{}`", model_path.display()))?;
    let model: ros_launch_manifest_model::SystemModel = serde_yaml_ng::from_str(&raw)
        .wrap_err_with(|| format!("parsing `{}`", model_path.display()))?;

    // phase-454 W3 / W7 — the same two refusals the seed makes, and for the same
    // reason: a descriptor composed from a QoS value this build could not read,
    // or from a contract a `qos_overrides.*` parameter disagrees with, describes
    // an image nobody is going to run. FATAL here rather than a refusal reason,
    // because unlike the seed this producer is reached only when a contract was
    // authored -- so a broken one is a configuration error, not a normal state.
    crate::cmd::entity_inventory::reject_unknown_qos_values(&model)?;
    crate::cmd::entity_inventory::reject_qos_override_divergence(&model)?;

    // phase-457 W0 (issue 1407) — read the metadata BEFORE the contract's own
    // predicate is consulted, so a metadata file the caller named and this
    // process cannot read is an ERROR rather than a silently poorer inventory.
    // That is the whole defect this wave closes, one level up: an absent half
    // that nothing announces reads exactly like a half that was empty.
    let mut metadata_inventory = match &args.metadata {
        Some(path) => Some(crate::cmd::entity_inventory::inventory_from_metadata_file(
            path,
        )?),
        None => None,
    };

    let inventory = crate::entity_inventory::EntityInventory::from_model(
        model_path.display().to_string(),
        &model,
    )
    // phase-457 W0 (issue 1407) — COMPOSE the metadata's component population
    // with the contract's, exactly as `nros ws entity-inventory` does and in
    // the same direction (metadata on the left, model on the right), so the two
    // producers of this schema derive over ONE inventory.
    //
    // Inside the `Some` arm and nowhere else: "no contract, no descriptor" is
    // W12's control and a metadata file must never resurrect a file for an
    // image that declared nothing — `merged_per_kind_max` is also documented as
    // safe only under this guard (issue 1402's `NotLaunched` rests on a model
    // with wiring existing).
    .map(|model_inv| match metadata_inventory.take() {
        Some(decl) => decl.merged_per_kind_max(&model_inv),
        None => model_inv,
    })
    // phase-454 (issue 1408) — the contract's `params:` ride along, exactly as
    // the configure-time producer and `nros build`'s seed attach them
    // (`cmd::entity_inventory`, `cmd::build::resolve_image`). Three composers
    // of one inventory must attach the same things, or the descriptor this
    // verb writes and the one a build writes describe different images
    // (issue 1228's shape).
    //
    // AFTER the merge, which is the order `cmd::entity_inventory::run` uses:
    // the declarations exist only in the model, so attaching them to the merged
    // result is the one placement that cannot depend on which side won.
    .map(|mut inv| {
        inv.set_param_declarations(crate::entity_inventory::ParamDeclarations::from_model(
            &model,
        ));
        inv
    });
    let Some(inventory) = inventory else {
        eprintln!(
            "nros ws sizing-descriptor: `{}` describes no wiring, so no sizing descriptor is \
             written and every consumer keeps its own defaults. State what each node creates in \
             the contract sidecar beside the launch file.",
            model_path.display()
        );
        return Ok(());
    };

    let written =
        crate::sizing_descriptor::write_for_model(&crate::sizing_descriptor::ModelImage {
            build_dir,
            entry,
            inventory: &inventory,
            target_triple: args.target_triple.clone(),
            host_build: args.host_build,
            heap_budget_bytes: args.heap_budget_bytes,
            rmw: args.rmw.clone(),
            bound_inventories: &args.bound_inventory,
            horizon: crate::sizing_descriptor::ModelHorizon::new(
                args.road.as_deref().unwrap_or(DEFAULT_MODEL_ROAD),
            ),
        })?;
    println!("{}", written.path.display());
    Ok(())
}

/// phase-457 W0.b — the STANDALONE LEAF write (issues 1407 / 1378).
///
/// A copy-out cmake project states what it creates in its own `system.toml`
/// `[[component]] entities` and has no bringup, no launch file and no
/// SystemModel — so [`write_from_model`] can never reach it, and it is the road
/// issue 1378 measured failing: `examples/qemu-armv7a-nuttx/{c,cpp}/action-server`
/// sized `ZPICO_MAX_QUERYABLES` to its three declared action services and died at
/// boot on the `/status` cache queryable that is the fourth.
///
/// ONE ROW PER `[[component]]`, not one row for the leaf. The counting rules read
/// the component set — `max_nodes` IS `components().len()` — and collapsing a
/// three-component leaf into one row would state a node table two slots short,
/// which is the same under-count W0 just closed on the model road.
///
/// **It stands down for a CARGO leaf**, and that is a property of the leaf rather
/// than of any file's state: such a leaf's descriptor is `nros sync`'s
/// (`write_for_leaf`), which has all three inventories and STATES the payload
/// class this producer must refuse. Twelve leaves in the tree carry both a
/// `CMakeLists.txt` and a `[package]` manifest, and for those the two producers
/// resolve the same path whenever the cmake build dir is the leaf's own `build/`
/// — overwriting the richer file with a poorer one is an UNDER-statement, the one
/// direction RFC-0100 D6 exists to keep out of this artifact.
fn write_from_leaf(args: &SizingDescriptorArgs, leaf: &std::path::Path) -> Result<()> {
    use crate::entity_inventory::{ComponentEntities, Declaration, EntityDecl, EntityInventory};

    let (Some(build_dir), Some(entry)) = (&args.build_dir, &args.entry) else {
        eyre::bail!("`--from-leaf` needs `--build-dir` and `--entry`");
    };
    let dir = leaf
        .canonicalize()
        .wrap_err_with(|| format!("leaf dir `{}`", leaf.display()))?;

    // A cargo leaf has a richer producer. Detected from the MANIFEST rather than
    // from whether a descriptor happens to be on disk: a file-state test is not
    // idempotent (our own output looks like somebody else's on the next
    // configure) and cannot distinguish the two producers at all.
    if is_cargo_leaf(&dir) {
        eprintln!(
            "nros ws sizing-descriptor: `{}` is a cargo leaf, so its sizing descriptor comes \
             from `nros sync` (which has the message-bound inventory this road does not). \
             Nothing written.",
            dir.display()
        );
        return Ok(());
    }

    let system = nros_orchestration_ir::leaf_system::read(&dir)
        .map_err(|e| eyre::eyre!(e))?
        .ok_or_else(|| {
            eyre::eyre!(
                "{}: declares no deployment -- write {} (RFC-0098 D3)",
                dir.display(),
                dir.join(nros_orchestration_ir::leaf_system::SYSTEM_TOML)
                    .display()
            )
        })?;

    // The DECLARATION is the opt-in, exactly as in `cmd::entity_facts::facts_from_leaf`.
    // A leaf nobody described gets NO descriptor: an all-refused file would move
    // the `[meta] basis` every consumer guards on in order to say nothing, which
    // is phase-454 W12's control and holds on this road too.
    let origin = system.origin_path().display().to_string();
    let mut rows = Vec::new();
    let mut any_declared = false;
    for (i, c) in system.components.iter().enumerate() {
        let declaration = match &c.entities {
            None => Declaration::Absent,
            Some(specs) => {
                any_declared = true;
                let mut decls = Vec::new();
                for spec in specs {
                    decls.extend(
                        EntityDecl::parse(spec)
                            .map_err(|e| eyre::eyre!("{origin}: entities entry `{spec}`: {e}"))?,
                    );
                }
                if decls.is_empty() {
                    Declaration::None
                } else {
                    Declaration::Stated(decls)
                }
            }
        };
        let name = c
            .name
            .clone()
            .or_else(|| c.class.clone())
            .unwrap_or_else(|| format!("component{i}"));
        rows.push(ComponentEntities {
            pkg: c.pkg.clone().unwrap_or_default(),
            component: name.clone(),
            class: c.class.clone().unwrap_or(name),
            declaration,
        });
    }
    if !any_declared {
        eprintln!(
            "nros ws sizing-descriptor: {origin} declares no entities, so no sizing descriptor \
             is written and every consumer keeps its own defaults. Add `entities = [...]` to its \
             `[[component]]` (RFC-0098 D8); the runtime's own service families are \
             `[system] features`."
        );
        return Ok(());
    }

    let mut inventory = EntityInventory::new(origin);
    for row in rows {
        inventory.insert(row);
    }
    // Issue 1270 — the runtime's own service families, from the SAME key a
    // bringup states them in. `InfraServices::from_features` is what
    // `facts_from_leaf` reads, so the two roads cannot come to disagree about
    // which spelling turns a family on.
    inventory.set_infra(crate::entity_inventory::InfraServices::from_features(
        &system.features,
        system.components.len(),
    ));

    let written =
        crate::sizing_descriptor::write_for_model(&crate::sizing_descriptor::ModelImage {
            build_dir,
            entry,
            inventory: &inventory,
            target_triple: args.target_triple.clone(),
            host_build: args.host_build,
            heap_budget_bytes: args.heap_budget_bytes,
            // The leaf states its own backend; `--rmw` still wins, for a caller
            // that resolved it more specifically (a cmake `-D`).
            rmw: args.rmw.clone().or_else(|| system.rmw.clone()),
            bound_inventories: &args.bound_inventory,
            horizon: crate::sizing_descriptor::ModelHorizon::for_leaf_declaration(
                args.road.as_deref().unwrap_or(DEFAULT_LEAF_ROAD),
            ),
        })?;
    println!("{}", written.path.display());
    Ok(())
}

/// Does this leaf have a `[package]` cargo manifest?
///
/// The discriminator for "`nros sync` writes this leaf's descriptor". A
/// `Cargo.toml` with only `[workspace]` is not a leaf's own package, so the
/// section is what is read rather than the file's existence.
fn is_cargo_leaf(dir: &std::path::Path) -> bool {
    let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
        return false;
    };
    text.lines()
        .any(|l| l.trim_start().starts_with("[package]"))
}

/// The human report. One line per fact, and a refusal prints its reason.
fn summary(desc: &nros_sizing_descriptor::SizingDescriptor) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "entry {} -- status {}, basis {} (schema {})",
        desc.meta.entry,
        desc.meta.status.tag(),
        desc.meta.basis.tag(),
        desc.schema_version
    );
    let _ = writeln!(
        s,
        "  undeclared endpoints: {}",
        desc.meta.undeclared_endpoints()
    );
    let _ = writeln!(s, "  target:");
    let _ = writeln!(s, "    pointer_bytes     {}", desc.target.pointer_bytes());
    let _ = writeln!(s, "    max_align         {}", desc.target.max_align());
    let _ = writeln!(
        s,
        "    heap_budget_bytes {}",
        desc.target.heap_budget_bytes()
    );
    let _ = writeln!(s, "  image:");
    let _ = writeln!(s, "    node_count        {}", desc.image.node_count());
    let _ = writeln!(s, "    backend_count     {}", desc.image.backend_count());
    let _ = writeln!(s, "    subscriber_count  {}", desc.image.subscriber_count());
    let _ = writeln!(s, "  types:");
    let _ = writeln!(s, "    distinct_count    {}", desc.types.distinct_count());
    let _ = writeln!(s, "    max_fields        {}", desc.types.max_fields());
    let _ = writeln!(s, "    max_kinds         {}", desc.types.max_kinds());
    let _ = writeln!(s, "    max_nested_depth  {}", desc.types.max_nested_depth());
    let _ = writeln!(s, "  endpoints: {}", desc.endpoints.len());
    for ep in &desc.endpoints {
        let _ = writeln!(s, "    {} {} [{}]", ep.kind.tag(), ep.topic, ep.type_name);
        let _ = writeln!(
            s,
            "      history {} depth {} reliability {} durability {}",
            ep.history(),
            ep.depth(),
            ep.reliability(),
            ep.durability()
        );
        let _ = writeln!(
            s,
            "      registration_path {} wire_bound_bytes {} storage_bytes {}",
            ep.registration_path(),
            ep.wire_bound_bytes(),
            ep.storage_bytes()
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_sizing_descriptor::{
        Basis, Endpoint, EndpointKind, History, RegistrationPath, SizingDescriptor, Status, Target,
    };

    fn desc() -> SizingDescriptor {
        let mut d = SizingDescriptor::new("talker", Status::Partial, Basis::Contract);
        d.meta.set_undeclared_endpoints(Some(0));
        d.target = Target::new(Some(4), Some(8), None);
        d.target
            .refuse("heap_budget_bytes", "board states no memory rung");
        let mut ep = Endpoint::new(
            EndpointKind::Subscription,
            "std_msgs/msg/String",
            "/chatter",
        );
        ep.set_history(Some(History::KeepLast))
            .set_depth(Some(10))
            .set_registration_path(Some(RegistrationPath::Unbounded))
            .set_wire_bound_bytes(Some(1170))
            .set_storage_bytes(Some(12914));
        d.endpoints.push(ep);
        d
    }

    #[test]
    fn the_cmake_projection_hides_a_refused_field_behind_if_defined() {
        let out = crate::sizing_descriptor::to_cmake(&desc());
        assert!(
            out.contains("set(NROS_SIZING_TARGET_POINTER_BYTES 4)"),
            "{out}"
        );
        // The refused field has NO value variable at all, so a consumer's
        // `if(DEFINED ...)` is the only road to a number -- D6 in CMake's
        // vocabulary. The reason travels beside it.
        assert!(
            !out.contains("set(NROS_SIZING_TARGET_HEAP_BUDGET_BYTES "),
            "{out}"
        );
        assert!(
            out.contains("set(NROS_SIZING_TARGET_HEAP_BUDGET_BYTES_REFUSED "),
            "{out}"
        );
    }

    #[test]
    fn the_endpoint_columns_stay_aligned_when_a_field_is_refused() {
        // An empty cmake list element vanishes on the next `list()` operation,
        // which would shorten one column and mis-align every row after it. A
        // refused slot is the literal `REFUSED` for exactly that reason.
        let mut d = desc();
        let mut ka = Endpoint::new(EndpointKind::Subscription, "sensor_msgs/msg/Image", "/i");
        ka.set_history(Some(History::KeepAll))
            .refuse("depth", "history = keep_all")
            .refuse("storage_bytes", "depends on depth");
        d.endpoints.push(ka);
        d.sort_endpoints();
        let out = crate::sizing_descriptor::to_cmake(&d);
        let depths = out
            .lines()
            .find(|l| l.starts_with("set(NROS_SIZING_ENDPOINT_DEPTH "))
            .unwrap();
        assert!(depths.contains("REFUSED"), "{depths}");
        assert_eq!(depths.matches(';').count(), 1, "{depths}");
        assert!(out.contains("set(NROS_SIZING_ENDPOINT_COUNT 2)"), "{out}");
    }

    #[test]
    fn the_summary_prints_a_refusal_rather_than_a_blank() {
        let s = summary(&desc());
        assert!(s.contains("heap_budget_bytes refused"), "{s}");
        // Absent prints as `absent`, never as a blank -- a blank column reads
        // as a value nobody bothered to fill in, and those are the two states
        // `Fact` exists to keep apart.
        assert!(s.contains("max_fields        absent"), "{s}");
        assert!(s.contains("unbounded"), "{s}");
    }

    /// Every field default, so a test states only what it is about.
    fn args() -> SizingDescriptorArgs {
        SizingDescriptorArgs {
            descriptor: None,
            output_cmake: None,
            from_model: None,
            build_dir: None,
            entry: None,
            metadata: None,
            rmw: None,
            target_triple: None,
            host_build: false,
            heap_budget_bytes: None,
            // phase-457 W0.b — `None`, so every case exercises the PER-MODE default
            // the cmake callers rely on rather than overriding it.
            road: None,
            from_leaf: None,
            // phase-457-payload W2 -- no tables: the refusal naming 1393.
            bound_inventory: Vec::new(),
        }
    }

    #[test]
    fn a_missing_descriptor_is_an_error_not_an_empty_projection() {
        let dir = tempfile::tempdir().unwrap();
        let err = run(SizingDescriptorArgs {
            descriptor: Some(dir.path().join("nope.toml")),
            output_cmake: Some(dir.path().join("out.cmake")),
            ..args()
        })
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("no sizing descriptor"),
            "{err:#}"
        );
        assert!(!dir.path().join("out.cmake").exists());
    }

    /// phase-454 W14 — a model that describes no wiring writes NOTHING and
    /// exits 0.
    ///
    /// "No contract, no change" is W12's own control, and this is the only
    /// place it can be asserted on the model road: every consumer treats a
    /// NAMED-but-absent descriptor as a hard error, so a file written here for
    /// an image with nothing to say would have to be all-refused — which moves
    /// the `[meta] basis` every one of them guards on in order to say nothing.
    #[test]
    fn a_model_with_no_wiring_writes_no_descriptor_and_does_not_fail() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("system_model.yaml");
        // A REAL empty model, serialised from the shared type rather than
        // hand-written: a hand-written stub that fails to parse would red this
        // test for the wrong reason and read exactly like the right one.
        let empty = ros_launch_manifest_model::SystemModel::default();
        std::fs::write(&model, serde_yaml_ng::to_string(&empty).unwrap()).unwrap();
        let build_dir = dir.path().join("build");
        run(SizingDescriptorArgs {
            from_model: Some(model),
            build_dir: Some(build_dir.clone()),
            entry: Some("nobody".into()),
            ..args()
        })
        .expect("a model with no wiring is a normal state, not a broken configure");
        assert!(
            !nros_sizing_descriptor::descriptor_path(&build_dir, "nobody").exists(),
            "an image with nothing to declare must get no descriptor at all"
        );
    }

    /// A model that publishes `/chatter` from node `/talker`, so
    /// `EntityInventory::from_model` describes exactly ONE component.
    fn model_with_one_contracted_node() -> ros_launch_manifest_model::SystemModel {
        use ros_launch_manifest_model::{SystemModel, TopicWiring};
        let mut m = SystemModel::default();
        m.structure.topics.insert(
            "/chatter".to_string(),
            TopicWiring {
                msg_type: "std_msgs/msg/String".to_string(),
                publishers: vec!["/talker/pub_out".to_string()],
                subscribers: vec![],
            },
        );
        m
    }

    /// phase-457 W0 (issue 1407) — the model-written descriptor must derive over
    /// the SAME component set `nros ws entity-inventory` derives over.
    ///
    /// **The reproduction, and it fails before the composition exists.** The
    /// contract describes one node; `nano_ros_node_register()` put THREE
    /// components in `nros-metadata.json`, which is the population the node
    /// table has to hold. Deriving over the contract's set alone states
    /// `node_count = 1` for an image that creates three nodes, and a short
    /// `NROS_EXECUTOR_MAX_NODES` is `NodeError::NodeTableFull` at boot — the
    /// direction RFC-0100 D6 exists to keep a descriptor out of.
    ///
    /// The metadata carries no `entities` key (that producer retired in
    /// phase-412 and the field with it in phase-454 W9), so this is the LIVE
    /// shape of a cmake configure: rows whose declaration is `Absent` and whose
    /// only contribution is that they EXIST.
    #[test]
    fn a_component_the_contract_does_not_describe_still_counts_toward_the_node_table() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("system_model.yaml");
        std::fs::write(
            &model,
            serde_yaml_ng::to_string(&model_with_one_contracted_node()).unwrap(),
        )
        .unwrap();
        let metadata = dir.path().join("nros-metadata.json");
        std::fs::write(
            &metadata,
            r#"{"components":[
                 {"name":"talker","pkg":"talker_pkg","class":"talker_pkg::Talker"},
                 {"name":"worker","pkg":"worker_pkg","class":"worker_pkg::Worker"},
                 {"name":"relay","pkg":"relay_pkg","class":"relay_pkg::Relay"}
               ]}"#,
        )
        .unwrap();
        let build_dir = dir.path().join("build");
        run(SizingDescriptorArgs {
            from_model: Some(model),
            metadata: Some(metadata),
            build_dir: Some(build_dir.clone()),
            entry: Some("island".into()),
            host_build: true,
            rmw: Some("zenoh".into()),
            ..args()
        })
        .expect("a contract was authored, so a descriptor is written");
        let desc = nros_sizing_descriptor::read(&nros_sizing_descriptor::descriptor_path(
            &build_dir, "island",
        ))
        .expect("the descriptor was written");
        assert_eq!(
            desc.image.node_count().stated().copied(),
            Some(3),
            "the node table holds every REGISTERED component, not only the \
             contracted ones -- {:?}",
            desc.image.node_count()
        );
    }

    /// phase-457 W0 — the two producers derive over ONE inventory, checked by
    /// comparing the descriptor against the verb's own derivation.
    ///
    /// This is the invariant, not the node count: 1407's mechanism 1 is that the
    /// SETS differ, and any field either producer derives from the set inherits
    /// the difference. Comparing the composed `Derivation` with what the file
    /// says binds them without this test having to enumerate the fields.
    ///
    /// **What it cannot assert, measured here rather than assumed.** 1407's
    /// acceptance was written as "a component the metadata names and the
    /// contract does not makes the descriptor REFUSE". That shape no longer
    /// refuses on EITHER producer: issue 1402 reclassifies exactly it to
    /// `Declaration::NotLaunched`, which `derive` deliberately does not filter
    /// on, and phase-454 W9 retired the metadata `entities` key so every
    /// metadata row is `Absent` to begin with. So the composed refusal the two
    /// producers now share is unreachable from this input, and what the sharing
    /// actually buys is the COUNT below. If 1402's proxy for "launched" ever
    /// changes, this assertion is where the refusal reappears on both roads at
    /// once — which is the property the wave was for.
    #[test]
    fn the_descriptor_states_what_the_inventory_verb_derives() {
        let dir = tempfile::tempdir().unwrap();
        let m = model_with_one_contracted_node();
        let model = dir.path().join("system_model.yaml");
        std::fs::write(&model, serde_yaml_ng::to_string(&m).unwrap()).unwrap();
        let metadata = dir.path().join("nros-metadata.json");
        std::fs::write(
            &metadata,
            r#"{"components":[
                 {"name":"talker","pkg":"talker_pkg","class":"talker_pkg::Talker"},
                 {"name":"worker","pkg":"worker_pkg","class":"worker_pkg::Worker"}
               ]}"#,
        )
        .unwrap();

        // The verb's own composition, through the same two functions it calls.
        let decl = crate::cmd::entity_inventory::inventory_from_metadata_file(&metadata).unwrap();
        let model_inv =
            crate::entity_inventory::EntityInventory::from_model("m", &m).expect("wiring");
        let composed = decl.merged_per_kind_max(&model_inv);
        let knobs = composed.derive();
        let knobs = knobs.knobs().expect("the composed inventory derives");

        let build_dir = dir.path().join("build");
        run(SizingDescriptorArgs {
            from_model: Some(model),
            metadata: Some(metadata),
            build_dir: Some(build_dir.clone()),
            entry: Some("island".into()),
            host_build: true,
            ..args()
        })
        .unwrap();
        let desc = nros_sizing_descriptor::read(&nros_sizing_descriptor::descriptor_path(
            &build_dir, "island",
        ))
        .unwrap();
        assert_eq!(
            desc.image.node_count().stated().copied(),
            Some(knobs.max_nodes),
            "the descriptor and `ws entity-inventory` must derive over one set"
        );
        assert_eq!(
            desc.image.subscriber_count().stated().copied(),
            Some(knobs.max_subscribers),
        );
    }

    /// The control: with no `--metadata`, the model stands alone and nothing
    /// changes. A configure that registered nothing has no metadata file, and
    /// the contract's set is then the whole truth.
    #[test]
    fn with_no_metadata_the_contract_alone_still_answers() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("system_model.yaml");
        std::fs::write(
            &model,
            serde_yaml_ng::to_string(&model_with_one_contracted_node()).unwrap(),
        )
        .unwrap();
        let build_dir = dir.path().join("build");
        run(SizingDescriptorArgs {
            from_model: Some(model),
            build_dir: Some(build_dir.clone()),
            entry: Some("island".into()),
            host_build: true,
            ..args()
        })
        .expect("written");
        let desc = nros_sizing_descriptor::read(&nros_sizing_descriptor::descriptor_path(
            &build_dir, "island",
        ))
        .unwrap();
        assert_eq!(desc.image.node_count().stated().copied(), Some(1));
    }

    // --- W0.b: the standalone-leaf road (issues 1407 / 1378) ----------------

    /// Write a leaf whose `system.toml` declares `body`, and return the dir.
    ///
    /// The `CMakeLists.txt` is not decoration: `leaf_system::is_package_dir` is
    /// what separates a LEAF from a workspace BRINGUP, and a `system.toml` with
    /// no package manifest beside it is the latter — its images are chosen by the
    /// workspace builder, not by this reader. So a standalone cmake leaf is
    /// exactly "a `system.toml` beside a `CMakeLists.txt`", and that is the shape
    /// under test.
    fn leaf_with(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
        let leaf = dir.join("action-server");
        std::fs::create_dir_all(&leaf).unwrap();
        std::fs::write(leaf.join("system.toml"), body).unwrap();
        std::fs::write(
            leaf.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.22)\nproject(leaf LANGUAGES C)\n",
        )
        .unwrap();
        leaf
    }

    /// phase-457 W0.b — the road issue 1378 measured FAILING now gets a
    /// descriptor, and the fact it failed on is in it.
    ///
    /// `examples/qemu-armv7a-nuttx/{c,cpp}/action-server`'s shape: one component,
    /// one declared action server, no bringup and no SystemModel. The action
    /// server's `/status` publisher is `TRANSIENT_LOCAL` whatever anything
    /// declares, so it owes a cache queryable — the FOURTH slot on an image sized
    /// to three, which is the boot failure. `transient_local_publishers` is the
    /// ONE rule that counts it, and the point of this wave is that the rule can
    /// now be fed from a descriptor on this road instead of only from an env
    /// carrier.
    #[test]
    fn a_standalone_leaf_declaration_gets_a_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        let leaf = leaf_with(
            dir.path(),
            r#"
[system]
name = "nuttx_c_action_server"
rmw = "zenoh"

[[component]]
pkg = "nuttx_c_action_server"
class = "nuttx_c_action_server::ActionServer"
name = "fibonacci_action_server"
entities = ["action_server:example_interfaces/action/Fibonacci:/fibonacci"]

[image.qemu-armv7a-nuttx]
board = "qemu-armv7a-nuttx"
"#,
        );
        let build_dir = dir.path().join("build");
        run(SizingDescriptorArgs {
            from_leaf: Some(leaf),
            build_dir: Some(build_dir.clone()),
            entry: Some("c_action_server".into()),
            host_build: true,

            ..args()
        })
        .expect("a leaf that declares its entities gets a descriptor");

        let desc = nros_sizing_descriptor::read(&nros_sizing_descriptor::descriptor_path(
            &build_dir,
            "c_action_server",
        ))
        .expect("the descriptor was written");
        assert_eq!(desc.endpoints.len(), 1, "{:?}", desc.endpoints);
        assert_eq!(desc.endpoints[0].kind, EndpointKind::ActionServer);
        assert_eq!(desc.endpoints[0].topic, "/fibonacci");
        // Issue 1378's fact, through the one rule that owns it.
        assert_eq!(
            nros_sizing_descriptor::transient_local_publishers(&desc).stated(),
            Some(&1),
            "an action server owes one cache queryable for its `/status`"
        );
        assert_eq!(desc.image.node_count().stated().copied(), Some(1));
    }

    /// The payload class stays REFUSED, and the refusal names the LEAF's input
    /// rather than a SystemModel this road does not have.
    ///
    /// A C or C++ leaf has no `generated/` bound table, so `wire_bound_bytes` is
    /// as unavailable here as on the model road — but telling its author that
    /// "the resolved SystemModel carries no message-bound inventory" points at an
    /// artifact that does not exist on their road, which is the diagnostic
    /// failure issue 1033 records one layer down.
    #[test]
    fn the_leaf_roads_refusal_names_the_declaration_not_a_model() {
        let dir = tempfile::tempdir().unwrap();
        let leaf = leaf_with(
            dir.path(),
            r#"
[system]
name = "leaf"
rmw = "zenoh"

[[component]]
name = "n"
entities = ["subscription:std_msgs/msg/String:/chatter"]

[image.i]
board = "qemu-armv7a-nuttx"
"#,
        );
        let build_dir = dir.path().join("build");
        run(SizingDescriptorArgs {
            from_leaf: Some(leaf),
            build_dir: Some(build_dir.clone()),
            entry: Some("e".into()),
            host_build: true,

            ..args()
        })
        .unwrap();
        let desc =
            nros_sizing_descriptor::read(&nros_sizing_descriptor::descriptor_path(&build_dir, "e"))
                .unwrap();
        let why = desc.endpoints[0]
            .wire_bound_bytes()
            .refusal()
            .expect("the payload class is refused on this road")
            .to_string();
        assert!(why.contains("system.toml"), "{why}");
        assert!(why.contains("a standalone cmake leaf"), "{why}");
        assert!(
            !why.contains("resolved SystemModel"),
            "this road has no model, so its refusal must not name one: {why}"
        );
        // Still tracked, and by the same issue: the missing input is the bound
        // inventory either way.
        assert!(
            why.contains(crate::sizing_descriptor::MODEL_ONLY_ISSUE),
            "{why}"
        );

        // `registration_path` is refused on BOTH roads and for DIFFERENT reasons,
        // so the prose must not be shared. A model image has no one entry
        // language because it is several packages; a leaf is ONE package whose
        // language is perfectly well known, and what it cannot state is the CALL
        // SITE. Saying the model's reason here would name a fact that is not
        // missing, which is how a refusal stops being actionable.
        let reg = desc.endpoints[0]
            .registration_path()
            .refusal()
            .expect("the registration path is refused on this road")
            .to_string();
        assert!(reg.contains("CALL SITE"), "{reg}");
        assert!(
            !reg.contains("several packages"),
            "a standalone leaf is ONE package; that clause belongs to the model road: {reg}"
        );

        // And `--road`'s DEFAULT is this mode's, not the model mode's. Passing no
        // `--road` here is deliberate: the bug this pins was a single clap
        // `default_value`, so a test that supplied the road explicitly could
        // never see it. Found by running the verb by hand and reading the file —
        // every refusal said "a cmake entry" on a road with no entry at all.
        assert!(
            why.contains(DEFAULT_LEAF_ROAD),
            "the leaf mode's default road must be `{DEFAULT_LEAF_ROAD}`: {why}"
        );
        assert!(
            !why.contains(DEFAULT_MODEL_ROAD),
            "the MODEL mode's road must not leak into a leaf refusal: {why}"
        );
    }

    /// EVERY `[[component]]` is a row, because `max_nodes` IS the component
    /// count. Collapsing a three-component leaf into one row would state a node
    /// table two slots short — the same under-count W0 closed on the model road.
    #[test]
    fn each_leaf_component_is_its_own_row() {
        let dir = tempfile::tempdir().unwrap();
        let leaf = leaf_with(
            dir.path(),
            r#"
[system]
name = "leaf"
rmw = "zenoh"

[[component]]
name = "a"
entities = ["publisher:std_msgs/msg/String:/a"]

[[component]]
name = "b"
entities = ["subscription:std_msgs/msg/String:/b"]

[[component]]
name = "c"
entities = ["timer"]

[image.i]
board = "qemu-armv7a-nuttx"
"#,
        );
        let build_dir = dir.path().join("build");
        run(SizingDescriptorArgs {
            from_leaf: Some(leaf),
            build_dir: Some(build_dir.clone()),
            entry: Some("e".into()),
            host_build: true,
            ..args()
        })
        .unwrap();
        let desc =
            nros_sizing_descriptor::read(&nros_sizing_descriptor::descriptor_path(&build_dir, "e"))
                .unwrap();
        assert_eq!(desc.image.node_count().stated().copied(), Some(3));
        // The timer is a component row and NOT an endpoint: it carries no type
        // and no topic, so no endpoint table can key on it.
        assert_eq!(desc.endpoints.len(), 2, "{:?}", desc.endpoints);
    }

    /// The control W12 owns: a leaf that declares NOTHING gets NO file.
    ///
    /// An all-refused descriptor would move the `[meta] basis` every consumer
    /// guards on in order to say nothing, and `entity-facts --leaf` abstains on
    /// exactly this input — the two roads must agree about when there is no
    /// answer, not only about what the answer is.
    #[test]
    fn a_leaf_that_declares_no_entities_gets_no_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        let leaf = leaf_with(
            dir.path(),
            r#"
[system]
name = "leaf"
rmw = "zenoh"

[[component]]
name = "n"

[image.i]
board = "qemu-armv7a-nuttx"
"#,
        );
        let build_dir = dir.path().join("build");
        run(SizingDescriptorArgs {
            from_leaf: Some(leaf),
            build_dir: Some(build_dir.clone()),
            entry: Some("e".into()),
            host_build: true,
            ..args()
        })
        .expect("declaring nothing is a normal state, not a broken configure");
        assert!(
            !nros_sizing_descriptor::descriptor_path(&build_dir, "e").exists(),
            "an image with nothing to declare must get no descriptor at all"
        );
    }

    /// A CARGO leaf keeps `nros sync`'s descriptor, which states strictly more.
    ///
    /// Twelve leaves in the tree carry both a `CMakeLists.txt` and a `[package]`
    /// manifest, and for those the two producers can resolve the SAME path.
    /// Overwriting the richer file with one that refuses the whole payload class
    /// is an UNDER-statement — the direction RFC-0100 D6 exists to keep out of
    /// this artifact. Decided from the MANIFEST, not from whether a file happens
    /// to be on disk: a file-state test is not idempotent across configures.
    #[test]
    fn a_cargo_leaf_keeps_the_descriptor_sync_writes() {
        let dir = tempfile::tempdir().unwrap();
        let leaf = leaf_with(
            dir.path(),
            r#"
[system]
name = "leaf"
rmw = "zenoh"

[[component]]
name = "n"
entities = ["publisher:std_msgs/msg/String:/a"]

[image.i]
board = "qemu-armv7a-nuttx"
"#,
        );
        std::fs::write(
            leaf.join("Cargo.toml"),
            "[package]\nname = \"leaf\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        let build_dir = dir.path().join("build");
        run(SizingDescriptorArgs {
            from_leaf: Some(leaf),
            build_dir: Some(build_dir.clone()),
            entry: Some("e".into()),
            host_build: true,
            ..args()
        })
        .expect("standing down is not a failure");
        assert!(
            !nros_sizing_descriptor::descriptor_path(&build_dir, "e").exists(),
            "the cargo road's producer has every input this one lacks"
        );
    }

    /// Neither `--descriptor` nor `--from-model` is an error naming both.
    #[test]
    fn the_verb_names_both_modes_when_given_neither() {
        let err = run(args()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("--descriptor"), "{msg}");
        assert!(msg.contains("--from-model"), "{msg}");
    }
}
