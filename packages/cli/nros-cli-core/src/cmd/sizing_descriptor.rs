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

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr};

#[derive(Debug, ClapArgs)]
pub struct SizingDescriptorArgs {
    /// The descriptor to read. `<build>/nros/sizing/<entry>.toml`.
    #[arg(long, value_name = "PATH")]
    pub descriptor: PathBuf,

    /// Write the `include()`able CMake projection here.
    #[arg(long, value_name = "PATH")]
    pub output_cmake: Option<PathBuf>,
}

pub fn run(args: SizingDescriptorArgs) -> Result<()> {
    // A MISSING descriptor is an error here and not a shrug. The caller named a
    // path; "it wasn't there so I printed nothing" is the silent-default shape
    // RFC-0100 D6 exists to forbid, and cmake would `include()` an empty file
    // and size from its own literals believing it had been told.
    let desc =
        nros_sizing_descriptor::read(&args.descriptor).wrap_err("reading the sizing descriptor")?;

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

    #[test]
    fn a_missing_descriptor_is_an_error_not_an_empty_projection() {
        let dir = tempfile::tempdir().unwrap();
        let err = run(SizingDescriptorArgs {
            descriptor: dir.path().join("nope.toml"),
            output_cmake: Some(dir.path().join("out.cmake")),
        })
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("no sizing descriptor"),
            "{err:#}"
        );
        assert!(!dir.path().join("out.cmake").exists());
    }
}
