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

    /// `--from-model`: the image's build dir. The descriptor lands at
    /// `<build-dir>/nros/sizing/<entry>.toml`.
    #[arg(long, value_name = "DIR")]
    pub build_dir: Option<PathBuf>,

    /// `--from-model`: the entry name, which becomes `[meta] entry`.
    #[arg(long, value_name = "NAME")]
    pub entry: Option<String>,

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

    /// `--from-model`: the road, in prose, for every refusal written.
    #[arg(long, value_name = "PROSE", default_value = "a cmake entry")]
    pub road: String,
}

pub fn run(args: SizingDescriptorArgs) -> Result<()> {
    if let Some(model) = &args.from_model {
        return write_from_model(&args, model);
    }
    let Some(descriptor) = &args.descriptor else {
        eyre::bail!(
            "pass either `--descriptor <path>` to read one or `--from-model <path>` to write one"
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

    let Some(inventory) = crate::entity_inventory::EntityInventory::from_model(
        model_path.display().to_string(),
        &model,
    ) else {
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
            road: &args.road,
        })?;
    println!("{}", written.path.display());
    Ok(())
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
            .set_registration_path(Some(RegistrationPath::RustTypedSchemaless))
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
        assert!(s.contains("rust_typed_schemaless"), "{s}");
    }

    /// Every field default, so a test states only what it is about.
    fn args() -> SizingDescriptorArgs {
        SizingDescriptorArgs {
            descriptor: None,
            output_cmake: None,
            from_model: None,
            build_dir: None,
            entry: None,
            rmw: None,
            target_triple: None,
            host_build: false,
            heap_budget_bytes: None,
            road: "a cmake entry".into(),
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

    /// Neither `--descriptor` nor `--from-model` is an error naming both.
    #[test]
    fn the_verb_names_both_modes_when_given_neither() {
        let err = run(args()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("--descriptor"), "{msg}");
        assert!(msg.contains("--from-model"), "{msg}");
    }
}
