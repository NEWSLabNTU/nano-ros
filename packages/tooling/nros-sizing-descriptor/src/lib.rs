//! RFC-0100 D4 — one sizing descriptor per entry, read BY PATH.
//!
//! ```text
//!   <build>/nros/sizing/<entry>.toml
//! ```
//!
//! `nros sync` writes it. Every consumer RFC-0100 D5 names — the executor, zenoh,
//! XRCE, Cyclone, uORB, cffi — reads it through THIS crate. That is the "reader
//! crate the consumers share" phase-454 W4 asks for, and it is a crate rather
//! than a convention because six hand-written TOML parsers over one schema is six
//! answers to "what does an absent `depth` mean".
//!
//! # Why a file and not an environment variable
//!
//! Env transport produced two of this tree's standing defects:
//!
//! * **issue 0460** — `nros_cargo_build.cmake` publishes knobs with
//!   `set(ENV{…})`, which touches only the configure-time process. The Zephyr C
//!   lane re-bakes them into its own command; `rust_cargo_application` builds its
//!   own and inherits nothing. So a knob reached one lane and not the other, and
//!   when the two disagreed it was also an ABI split.
//! * **issue 0491** — cargo compares an env value as TEXT, and one directory has
//!   three spellings here. `rerun-if-env-changed` on a variable that names a path
//!   rebuilt the world forever.
//!
//! A file answers both: one artifact, one road, and a `rerun-if-changed` edge on
//! its CONTENT. It is also the only transport that can carry per-endpoint
//! structure without encoding it in a string, which is what a `(kind, type,
//! topic) -> {depth, history, registration_path, …}` table needs.
//!
//! # What it must get right
//!
//! **Per-FIELD status, not one global status** (D6). See [`Fact`]. A `keep_all`
//! subscription refuses its depth-derived fields and degrades nothing else.
//!
//! **`[target]` comes from the BOARD, never from a host build script** (D1).
//! Build scripts run for the host (phase-118-E), so a `size_of` there answers the
//! wrong question on a cross build. See [`Target`].
//!
//! **`registration_path` is REQUIRED** (issue 1319). A subscription's slot size
//! depends on it and nothing else can supply it. See [`RegistrationPath`].
//!
//! **Demand is published UNFLOORED** (D7, issues 1015 + 1033). Zero is a
//! legitimate demand. Nothing in this crate floors anything, and that is a
//! deliberate absence: 1015's floor landed in a shared derivation a day before
//! 1033's fix and silently defeated it, with every knob gate green.
//!
//! # Portability
//!
//! The descriptor carries NO absolute path — issue 0320's rule, and the reason
//! is the same one that made SystemModels content-addressed: two checkouts of one
//! tree at different paths must produce byte-identical artifacts, or every
//! freshness comparison is a false negative. The schema has no path field, and
//! [`portability_violation`] lets the producer assert the rendered output against
//! the checkout directories it actually read from — the only exact answer, since
//! a ROS topic is an absolute-looking string too.

pub mod fact;
mod render;
mod schema;
pub mod vocabulary;

use std::path::{Path, PathBuf};

pub use fact::Fact;
pub use render::{portability_violation, render};
pub use schema::{Endpoint, Image, Meta, Params, Policy, SizingDescriptor, Target, Types};
pub use vocabulary::{
    Basis, CapacityNeed, Durability, EndpointKind, History, RegistrationPath, Reliability, Status,
};

/// How many TRANSIENT_LOCAL publishers this image declares.
///
/// phase-455 W5 / issue 1341. **ONE formula**, because it has TWO consumers
/// that must agree or the image does not boot: `nros-rmw-zenoh` sizes the
/// retention pool from it, and `nros-zpico-build` adds it to the queryable
/// table — a transient-local publisher retains its last sample AND declares a
/// queryable to serve that sample to a late joiner. Issue 1025 is what a second
/// derivation of one number costs; this is the first place both could reach.
///
/// # What counts, and the row that is not a `durability` field
///
/// * a `publisher` row whose `durability` is `transient_local`, and
/// * every `action_server` row, ONE each, whatever its own durability says.
///
/// The second is not a guess. `nros-node` creates an action server's
/// `<action>/_action/status` publisher itself with
/// `rcl_action_qos_profile_status_default` — KEEP_LAST(1) / RELIABLE /
/// TRANSIENT_LOCAL — and that publisher is BELOW the declaration: it appears in
/// no `[[endpoint]]` row because no launch file mentions it. Counting only the
/// publisher rows answers 0 for an action-server image, which is exactly the
/// image issue 1341 is about, and exactly the image whose queryable table then
/// fills at boot.
///
/// # The three answers, and why `Absent` is not zero
///
/// * [`Fact::Stated`] — every publisher row stated a durability, so the count is
///   exact. Zero is a legitimate answer and means this image pays nothing.
/// * [`Fact::Refused`] — some publisher row states no durability. A count over
///   the rows that DID answer is not a bound on the row that stayed silent, and
///   an under-sized pool here is `create_publisher` failing at boot. The prose
///   names the row.
/// * [`Fact::Absent`] — the descriptor has no endpoint rows at all, so there is
///   no declaration to read. Consumers keep their builtin.
///
/// Nothing here is floored (D7): whether zero is a legal SIZE is a property of
/// the consumer's storage.
pub fn transient_local_publishers(desc: &SizingDescriptor) -> Fact<usize> {
    transient_local_publishers_over(desc.endpoints.iter().map(|e| TlRow {
        kind: e.kind,
        durability: e.durability(),
        topic: &e.topic,
        type_name: &e.type_name,
    }))
}

/// One row of the [`transient_local_publishers`] rule.
///
/// Issue 1378 — the rule needed a SECOND caller, and the descriptor is not it.
/// A cmake / Zephyr / NuttX entry has no sizing descriptor at all (issue 1393),
/// so the only rows it can offer are its DECLARED ENTITIES. Without a row shape
/// to offer them as, such a caller's only alternative is to re-implement "an
/// action server has a transient-local `/status` publisher" somewhere else,
/// which is issue 1025's defect exactly: one number, two derivations, agreeing
/// until the day they do not.
#[derive(Debug, Clone)]
pub struct TlRow<'a> {
    pub kind: EndpointKind,
    pub durability: Fact<Durability>,
    pub topic: &'a str,
    pub type_name: &'a str,
}

/// [`transient_local_publishers`] over any source of rows — **the rule itself**.
///
/// No rows means [`Fact::Absent`]: "there is no declaration to read", never
/// "this image has no transient-local publisher". The callers differ only in
/// where their rows come from.
pub fn transient_local_publishers_over<'a, I>(rows: I) -> Fact<usize>
where
    I: IntoIterator<Item = TlRow<'a>>,
{
    let mut count = 0usize;
    let mut any = false;
    for e in rows {
        any = true;
        match e.kind {
            EndpointKind::ActionServer => count += 1,
            EndpointKind::Publisher => match e.durability {
                Fact::Stated(Durability::TransientLocal) => count += 1,
                Fact::Stated(Durability::Volatile) => {}
                f => {
                    return Fact::Refused(format!(
                        "publisher {} ({}) states no `durability`: {}. A count over the \
                         rows that answered is not a bound on the row that did not",
                        e.topic,
                        e.type_name,
                        f.refusal().unwrap_or("nothing derived it"),
                    ));
                }
            },
            _ => {}
        }
    }
    if !any {
        return Fact::Absent;
    }
    Fact::Stated(count)
}

/// [`transient_local_publishers`] for a build script, straight off
/// [`DESCRIPTOR_ENV`].
///
/// `Fact::Absent` when no descriptor was named — the undeclared road, which is
/// every bare `cargo build` and every leaf that has not run `nros sync`.
pub fn transient_local_publishers_from_build_env() -> Result<Fact<usize>, DescriptorError> {
    Ok(match from_build_env()? {
        Some(desc) => transient_local_publishers(&desc),
        None => Fact::Absent,
    })
}

/// The schema version this reader understands.
///
/// Bumped whenever a consumer that kept reading an older file would size from a
/// number whose MEANING moved — the rule that took the entity inventory to 5 over
/// `history = keep_all`. A version this reader does not know is a refusal to
/// read, never a best effort.
pub const SCHEMA_VERSION: u32 = 1;

/// Where descriptors live under a build directory.
pub const SIZING_SUBDIR: &str = "nros/sizing";

/// The env variable a build naming a descriptor uses.
///
/// It names a PATH, and issue 0491 is why nothing watches the VARIABLE: the
/// content edge is on the file (see [`load_for_build_script`]). One directory has
/// three spellings in this tree and cargo compares an env value as text, so a
/// `rerun-if-env-changed` here would rebuild the world on a cosmetic difference
/// while missing an actual edit to the descriptor.
pub const DESCRIPTOR_ENV: &str = "NROS_SIZING_DESCRIPTOR";

/// `<build_dir>/nros/sizing/<entry>.toml`.
///
/// The ONE place this path is computed. `model_location` exists for exactly this
/// reason one artifact over — three consumers each derived the SystemModel path
/// independently and two of them drifted.
pub fn descriptor_path(build_dir: &Path, entry: &str) -> PathBuf {
    build_dir.join(SIZING_SUBDIR).join(format!("{entry}.toml"))
}

/// What went wrong reading one.
#[derive(Debug)]
pub enum DescriptorError {
    /// The file is not there. Separate from [`Self::Read`] because a consumer
    /// legitimately has no descriptor — an image that declares nothing — and must
    /// fall back to its own defaults rather than fail the build.
    Missing(PathBuf),
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    /// TOML that does not parse, a key the schema does not have, a value the
    /// vocabulary does not know. **Never a fallback**: a descriptor exists, so a
    /// consumer that defaulted here would be sizing from numbers a user believes
    /// they supplied.
    Parse { path: PathBuf, message: String },
    /// The file parses and breaks a rule of the format — a field both stated and
    /// refused, a refusal naming a field that does not exist, a schema version
    /// this reader does not know.
    Invalid { path: PathBuf, message: String },
}

impl std::fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DescriptorError::Missing(p) => {
                write!(f, "no sizing descriptor at `{}`", p.display())
            }
            DescriptorError::Read { path, source } => {
                write!(f, "read sizing descriptor `{}`: {source}", path.display())
            }
            DescriptorError::Parse { path, message } => write!(
                f,
                "sizing descriptor `{}` does not parse: {message}\n\
                 It is a generated artifact -- re-run `nros sync` rather than editing it.",
                path.display()
            ),
            DescriptorError::Invalid { path, message } => {
                write!(f, "sizing descriptor `{}`: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for DescriptorError {}

impl DescriptorError {
    /// Is this the "there isn't one" case a consumer may fall back on?
    ///
    /// Spelled as a predicate so a caller writes `if e.is_missing()` instead of
    /// matching, and so every other variant is loud by construction: a
    /// `_ => default()` arm over this enum is the silent-default shape D6
    /// forbids.
    pub fn is_missing(&self) -> bool {
        matches!(self, DescriptorError::Missing(_))
    }
}

/// Parse descriptor TEXT. `path` is used for diagnostics only.
pub fn parse(text: &str, path: &Path) -> Result<SizingDescriptor, DescriptorError> {
    let desc: SizingDescriptor = toml::from_str(text).map_err(|e| DescriptorError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    desc.validate()
        .map_err(|message| DescriptorError::Invalid {
            path: path.to_path_buf(),
            message,
        })?;
    Ok(desc)
}

/// Read one from disk.
pub fn read(path: &Path) -> Result<SizingDescriptor, DescriptorError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(DescriptorError::Missing(path.to_path_buf()));
        }
        Err(source) => {
            return Err(DescriptorError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    parse(&text, path)
}

/// Read one from a build script, with the rebuild edge acceptance asks for.
///
/// Emits `cargo::rerun-if-changed=<path>` for the file's CONTENT, which is the
/// whole point of a file transport — **whether or not the file is there yet**,
/// because CREATION is the edge that matters: the first `nros sync` after a
/// build is what turns an image's defaults into its declaration, and a watch
/// that only names paths that already exist cannot see it.
///
/// An earlier version of this function emitted the line only after an
/// `exists()` check, citing issue 0490's permanently-dirty unit. Both halves of
/// that were measured on cargo 1.98.1, against a build script emitting this one
/// line, with a side effect as the signal (a `cargo::warning` is REPLAYED for a
/// fresh unit, so counting warnings measures nothing):
///
/// | build | watch after `exists()` | watch either way |
/// | --- | --- | --- |
/// | 1, descriptor absent, cold | RAN | RAN |
/// | 2, descriptor absent | fresh | RAN |
/// | 3, descriptor **created** | **fresh** | **RAN** |
/// | 4, descriptor present, unchanged | fresh | fresh |
/// | 5, descriptor **edited** | **fresh** | **RAN** |
///
/// So the old shape did not merely miss the creation — it never looked at the
/// file again at all, and a later EDIT was invisible too. And the dirt the
/// check was avoiding is bounded exactly by the state that wants re-checking:
/// column 2 re-runs while the descriptor is absent and goes quiet on the build
/// after it appears. `scripts/check-build-rs-rerun-paths.py` polices STATIC
/// literal paths in a `build.rs`, which this is not — it is the path a road
/// pointed this build at, and naming it is how the road's own artifact gets an
/// edge.
///
/// A path that was named and is not there is still an error rather than a silent
/// skip: somebody pointed the build at a descriptor, and "it wasn't there so I
/// used my defaults" is the shape D6 exists to forbid. The watch is emitted
/// first so that the answer survives the refusal — it is a statement about the
/// PATH, not about the file that may or may not be at it.
pub fn load_for_build_script(path: &Path) -> Result<SizingDescriptor, DescriptorError> {
    load_for_build_script_emitting(path, &mut |line| println!("{line}"))
}

/// [`load_for_build_script`] with the cargo directives handed to `emit`.
///
/// Split out so a test can assert that a MISSING descriptor is watched anyway —
/// the ordering above is the whole of this function, and an ordering nothing
/// exercises is an ordering that gets re-swapped by the next reader of issue
/// 0490. Printing to stdout is not observable from a unit test; a closure is.
fn load_for_build_script_emitting(
    path: &Path,
    emit: &mut dyn FnMut(&str),
) -> Result<SizingDescriptor, DescriptorError> {
    emit(&format!("cargo::rerun-if-changed={}", path.display()));
    // No `exists()` guard: `read` already reports a NotFound as
    // `Missing`, so a second probe would only add a window in which the
    // answer changes between the two.
    read(path)
}

/// The descriptor this build was pointed at, if any.
///
/// `Ok(None)` when [`DESCRIPTOR_ENV`] is unset or empty — the ordinary case for
/// a build nobody has run `nros sync` for, and the one where a consumer keeps
/// its own defaults. Every other outcome is an error the caller must surface.
pub fn from_build_env() -> Result<Option<SizingDescriptor>, DescriptorError> {
    match std::env::var(DESCRIPTOR_ENV) {
        Ok(v) if !v.trim().is_empty() => load_for_build_script(Path::new(v.trim())).map(Some),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn island() -> SizingDescriptor {
        let mut d = SizingDescriptor::new("talker", Status::Derived, Basis::Contract);
        d.meta.set_undeclared_endpoints(Some(0));
        d.target = Target::new(Some(4), Some(8), Some(65536));
        let mut sub = Endpoint::new(
            EndpointKind::Subscription,
            "std_msgs/msg/String",
            "/chatter",
        );
        sub.set_history(Some(History::KeepLast))
            .set_depth(Some(10))
            .set_reliability(Some(Reliability::Reliable))
            .set_durability(Some(Durability::Volatile))
            .set_registration_path(Some(RegistrationPath::Unbounded))
            .set_storage_bytes(Some(280))
            .set_wire_bound_bytes(Some(1170));
        d.endpoints.push(sub);
        d.types = Types::new(Some(7), Some(12), Some(48), Some(3));
        d.policy = Policy::new(Some(64), Some(4096), Some(1));
        d.image = Image::new(Some(2), Some(1), Some(3));
        d
    }

    /// phase-455 W5 / issue 1341 — the ONE count two pools size from.
    #[test]
    fn transient_local_publishers_counts_tl_publishers_and_every_action_server() {
        let mut d = island();
        // The island has one VOLATILE subscription and nothing else.
        assert_eq!(transient_local_publishers(&d), Fact::Stated(0));

        let mut tl = Endpoint::new(EndpointKind::Publisher, "std_msgs/msg/String", "/latched");
        tl.set_durability(Some(Durability::TransientLocal));
        d.endpoints.push(tl);
        assert_eq!(transient_local_publishers(&d), Fact::Stated(1));

        let mut vol = Endpoint::new(EndpointKind::Publisher, "std_msgs/msg/String", "/chatter");
        vol.set_durability(Some(Durability::Volatile));
        d.endpoints.push(vol);
        assert_eq!(
            transient_local_publishers(&d),
            Fact::Stated(1),
            "a volatile publisher costs nothing"
        );

        // An action server's `/status` publisher is BELOW the declaration —
        // nothing states its durability because nothing states the endpoint —
        // so the row counts one on its kind alone. This is the arm issue 1341's
        // image depends on.
        d.endpoints.push(Endpoint::new(
            EndpointKind::ActionServer,
            "example_interfaces/action/Fibonacci",
            "/fibonacci",
        ));
        assert_eq!(transient_local_publishers(&d), Fact::Stated(2));
    }

    /// A publisher that states no durability REFUSES the count, naming itself:
    /// a total over the rows that answered is not a bound on the row that did
    /// not, and both consumers size a pool whose shortfall is a boot failure.
    #[test]
    fn a_publisher_with_no_durability_refuses_the_count() {
        let mut d = island();
        d.endpoints.push(Endpoint::new(
            EndpointKind::Publisher,
            "std_msgs/msg/String",
            "/unstated",
        ));
        match transient_local_publishers(&d) {
            Fact::Refused(reason) => assert!(
                reason.contains("/unstated"),
                "the refusal must name the row: {reason}"
            ),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// An EMPTY descriptor is `Absent`, never `Stated(0)`. A consumer must be
    /// able to tell "this image declares no transient-local publisher" from
    /// "nobody described this image", because the first is a pool of zero and
    /// the second is the builtin.
    #[test]
    fn no_endpoint_rows_is_absent_rather_than_zero() {
        let d = SizingDescriptor::new("bare", Status::Derived, Basis::Contract);
        assert_eq!(transient_local_publishers(&d), Fact::Absent);
    }

    #[test]
    fn a_derived_descriptor_round_trips() {
        let d = island();
        let text = render(&d);
        let back = parse(&text, Path::new("talker.toml")).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn render_is_byte_stable_across_repeated_calls() {
        let d = island();
        assert_eq!(render(&d), render(&d));
    }

    #[test]
    fn a_keep_all_endpoint_refuses_its_depth_and_degrades_nothing_else() {
        // RFC-0100 D6, the trigger with a live defect behind it. The image's
        // TYPE table and the other endpoint's depth must survive intact.
        let mut d = island();
        let mut ep = Endpoint::new(
            EndpointKind::Subscription,
            "sensor_msgs/msg/Image",
            "/image",
        );
        ep.set_history(Some(History::KeepAll))
            .set_wire_bound_bytes(Some(1170))
            .refuse(
                "depth",
                "history = keep_all on subscription /image: a KEEP_ALL queue has no static bound",
            )
            .refuse("storage_bytes", "depends on `depth`, which is refused");
        d.endpoints.push(ep);
        d.sort_endpoints();

        let back = parse(&render(&d), Path::new("t.toml")).unwrap();
        let keep_all = back
            .endpoints
            .iter()
            .find(|e| e.topic == "/image")
            .expect("the keep_all endpoint survived");
        assert_eq!(keep_all.depth().tag(), "refused");
        assert!(keep_all.depth().stated().is_none());
        assert!(keep_all.depth().refusal().unwrap().contains("keep_all"));
        // Not degraded: the same row's history IS readable, and so is the
        // image's type table.
        assert_eq!(keep_all.history().stated(), Some(&History::KeepAll));
        assert_eq!(keep_all.wire_bound_bytes().stated(), Some(&1170));
        assert_eq!(back.types.distinct_count().stated(), Some(&7));
        let chatter = back
            .endpoints
            .iter()
            .find(|e| e.topic == "/chatter")
            .unwrap();
        assert_eq!(chatter.depth().stated(), Some(&10));
    }

    #[test]
    fn absent_and_refused_are_different_answers() {
        let mut d = SizingDescriptor::new("x", Status::Partial, Basis::Contract);
        d.target
            .refuse("pointer_bytes", "board states no rustc target");
        let back = parse(&render(&d), Path::new("x.toml")).unwrap();
        assert_eq!(back.target.pointer_bytes().tag(), "refused");
        // `max_align` was never mentioned. That is ABSENT, and it must not read
        // as refused -- there is no prose, because there was no event.
        assert_eq!(back.target.max_align().tag(), "absent");
        assert!(back.target.max_align().refusal().is_none());
        // Neither yields a number. That is the only thing they share, and it is
        // the thing that keeps a consumer honest.
        assert!(back.target.pointer_bytes().stated().is_none());
        assert!(back.target.max_align().stated().is_none());
    }

    #[test]
    fn zero_is_a_demand_and_survives_unfloored() {
        // RFC-0100 D7 / issues 1015 + 1033. A floor anywhere in this crate would
        // silently defeat the consumer for which zero is the right answer, and
        // every knob gate would stay green because the number was delivered
        // faithfully.
        let mut d = SizingDescriptor::new("empty", Status::Derived, Basis::Contract);
        d.meta.set_undeclared_endpoints(Some(0));
        d.types = Types::new(Some(0), None, None, None);
        let back = parse(&render(&d), Path::new("e.toml")).unwrap();
        assert_eq!(back.types.distinct_count().stated(), Some(&0));
        assert_eq!(back.meta.undeclared_endpoints().stated(), Some(&0));
    }

    #[test]
    fn the_image_counts_survive_a_round_trip_and_refuse_one_at_a_time() {
        // phase-454 W6.e. The three counts feed three different cffi pools, and
        // D6's rule is that a refusal degrades nothing else -- an image whose
        // backend nobody named must still get its node table sized.
        let mut d = island();
        d.image = Image::new(Some(2), None, Some(3));
        d.image.refuse(
            "backend_count",
            "the image names no rmw, so what it links \
                    is not known here",
        );
        let back = parse(&render(&d), Path::new("t.toml")).unwrap();
        assert_eq!(back.image.node_count().stated(), Some(&2));
        assert_eq!(back.image.subscriber_count().stated(), Some(&3));
        assert_eq!(back.image.backend_count().tag(), "refused");
        assert!(
            back.image
                .backend_count()
                .refusal()
                .unwrap()
                .contains("names no rmw")
        );
    }

    #[test]
    fn an_image_with_no_counts_reads_absent_not_zero() {
        // The distinction the whole `Fact` type exists for, at the section a
        // pool size is read from: a consumer that saw `0` here would build a
        // zero-slot registry for an image nobody measured.
        let d = SizingDescriptor::new("x", Status::Refused, Basis::Closure);
        let back = parse(&render(&d), Path::new("x.toml")).unwrap();
        assert_eq!(back.image.node_count().tag(), "absent");
        assert_eq!(back.image.backend_count().tag(), "absent");
        assert!(back.image.subscriber_count().stated().is_none());
    }

    #[test]
    fn a_zero_subscriber_count_is_a_demand_and_reaches_the_consumer() {
        // Issue 1033's half of D7: a pub-only image demands ZERO subscriber
        // slots and that is the answer, worth 1 KiB a slot in the cffi pool.
        // Nothing here floors it; whether zero is a legal SIZE is decided at
        // the pool that names the knob.
        let mut d = island();
        d.image = Image::new(Some(1), Some(1), Some(0));
        let back = parse(&render(&d), Path::new("p.toml")).unwrap();
        assert_eq!(back.image.subscriber_count().stated(), Some(&0));
    }

    // --- `[params]`, issue 1408 ---------------------------------------------

    /// The island's parameter store, fully derived: three declarations on two
    /// nodes, one of them a `string`.
    fn with_params(d: &mut SizingDescriptor) {
        d.params
            .set_declared(Some(3))
            .set_max_parameters(Some(4))
            .set_max_param_name_len(Some(9))
            .set_needs_max_string_value_len(Some(CapacityNeed::NeededBy {
                node: "/a".into(),
                name: "label".into(),
            }))
            .set_needs_max_array_len(Some(CapacityNeed::Unused))
            .set_needs_max_byte_array_len(Some(CapacityNeed::Unused))
            .set_service_shape(Some("4:31:1:5:1:0:0:0:0,2:17:0:0:0:0:0:0:0".into()));
    }

    #[test]
    fn a_fully_stated_params_section_round_trips() {
        let mut d = island();
        with_params(&mut d);
        let text = render(&d);
        let back = parse(&text, Path::new("talker.toml")).unwrap();

        assert_eq!(back.params.declared().stated(), Some(&3));
        assert_eq!(back.params.max_parameters().stated(), Some(&4));
        assert_eq!(back.params.max_param_name_len().stated(), Some(&9));
        assert_eq!(
            back.params
                .needs_max_string_value_len()
                .stated()
                .and_then(|c| c.needed_by()),
            Some(("/a", "label"))
        );
        assert_eq!(
            back.params.needs_max_array_len().stated(),
            Some(&CapacityNeed::Unused)
        );
        assert_eq!(
            back.params.needs_max_byte_array_len().stated(),
            Some(&CapacityNeed::Unused)
        );
        assert_eq!(
            back.params.service_shape().stated().map(String::as_str),
            Some("4:31:1:5:1:0:0:0:0,2:17:0:0:0:0:0:0:0")
        );

        // The typed value and the BYTES both survive: an artifact that is
        // compared and written write-if-changed has to render identically from
        // what it parsed, or every freshness comparison is a false negative.
        assert_eq!(back, d);
        assert_eq!(render(&back), text);
    }

    /// Every field of `[params]` must be in `FIELDS`, because `FIELDS` is what
    /// the writer emits and what `refuse()` and the parse rules police. A field
    /// present in the struct and missing from the list is silently DROPPED on
    /// render while every accessor still reads it in memory — the shape that
    /// looks like it works until somebody re-reads the file.
    #[test]
    fn every_params_field_is_rendered_and_read_back() {
        let mut d = SizingDescriptor::new("p", Status::Derived, Basis::Contract);
        with_params(&mut d);
        let text = render(&d);
        for field in [
            "declared",
            "max_parameters",
            "max_param_name_len",
            "needs_max_string_value_len",
            "needs_max_array_len",
            "needs_max_byte_array_len",
            "service_shape",
        ] {
            assert!(
                text.contains(&format!("\n{field} = ")),
                "[params] did not render `{field}`:\n{text}"
            );
        }
        assert_eq!(parse(&text, Path::new("p.toml")).unwrap(), d);
    }

    #[test]
    fn a_refused_params_field_yields_no_value_and_degrades_nothing_else() {
        let mut d = island();
        with_params(&mut d);
        // The model road has the declarations and not the token: a shape is
        // per NODE and an image built from a probe has no node list to walk.
        d.params.set_service_shape(None).refuse(
            "service_shape",
            "no per-node parameter shape on this road -- issue 1393",
        );
        let back = parse(&render(&d), Path::new("t.toml")).unwrap();

        assert_eq!(back.params.service_shape().tag(), "refused");
        assert!(back.params.service_shape().stated().is_none());
        assert!(
            back.params
                .service_shape()
                .refusal()
                .unwrap()
                .contains("1393")
        );
        // D6: a refusal degrades nothing else, in this section or any other.
        assert_eq!(back.params.max_parameters().stated(), Some(&4));
        assert_eq!(back.types.distinct_count().stated(), Some(&7));
    }

    #[test]
    fn a_params_field_both_stated_and_refused_is_a_contradiction() {
        let mut d = island();
        with_params(&mut d);
        let text = format!(
            "{}\n[params.refused]\nmax_parameters = \"nobody counted\"\n",
            render(&d)
        );
        let err = parse(&text, Path::new("t.toml")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("both states and refuses"), "{msg}");
        assert!(msg.contains("max_parameters"), "{msg}");
    }

    #[test]
    fn a_params_refusal_naming_no_field_is_an_error() {
        // A refusal nobody can read is worse than none -- the consumer defaults
        // silently and the producer believes it warned.
        let d = SizingDescriptor::new("p", Status::Partial, Basis::Contract);
        let text = format!(
            "{}\n[params.refused]\nmax_paramters = \"typo\"\n",
            render(&d)
        );
        let err = parse(&text, Path::new("p.toml")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("max_paramters"), "{msg}");
        assert!(msg.contains("max_parameters"), "{msg}");
    }

    /// `unused` is a STATEMENT and an absent key is not, asserted in ONE test so
    /// the distinction cannot collapse silently. An image that declares
    /// parameters and uses no array type has told the board its
    /// `MAX_ARRAY_LEN` buys nothing; an image that declares none has said
    /// nothing at all, and the consumer must keep its builtin there.
    #[test]
    fn unused_is_stated_and_an_absent_field_is_not() {
        let mut d = SizingDescriptor::new("p", Status::Partial, Basis::Contract);
        d.params
            .set_declared(Some(1))
            .set_needs_max_array_len(Some(CapacityNeed::Unused));
        let back = parse(&render(&d), Path::new("p.toml")).unwrap();

        let unused = back.params.needs_max_array_len();
        let absent = back.params.needs_max_byte_array_len();
        assert_eq!(unused.stated(), Some(&CapacityNeed::Unused));
        assert_eq!(unused.tag(), "stated");
        assert_eq!(absent.tag(), "absent");
        assert_eq!(absent.stated(), None);
        assert!(absent.refusal().is_none());
        assert_ne!(unused.tag(), absent.tag());
    }

    #[test]
    fn an_unknown_capacity_need_spelling_refuses_rather_than_guessing() {
        let mut d = SizingDescriptor::new("p", Status::Partial, Basis::Contract);
        d.params.set_needs_max_array_len(Some(CapacityNeed::Unused));
        let text = render(&d).replace("\"unused\"", "\"maybe\"");
        let err = parse(&text, Path::new("p.toml")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("maybe"), "{msg}");
        assert!(msg.contains("unused"), "{msg}");
        assert!(msg.contains("needed_by:"), "{msg}");
    }

    /// Backward compatibility: `[params]` is purely ADDITIVE, so
    /// `SCHEMA_VERSION` stays 1 and a descriptor written before it exists parses
    /// unchanged. Every accessor then answers `Absent`, which is the true
    /// answer — the file's writer knew nothing about parameters.
    #[test]
    fn a_descriptor_with_no_params_section_parses_and_reads_absent() {
        let text = "\
schema_version = 1

[meta]
entry = \"legacy\"
status = \"derived\"
basis = \"contract\"
";
        let back = parse(text, Path::new("legacy.toml")).unwrap();
        assert_eq!(back.params, Params::default());
        assert_eq!(back.params.declared().tag(), "absent");
        assert_eq!(back.params.max_parameters().tag(), "absent");
        assert_eq!(back.params.max_param_name_len().tag(), "absent");
        assert_eq!(back.params.needs_max_string_value_len().tag(), "absent");
        assert_eq!(back.params.needs_max_array_len().tag(), "absent");
        assert_eq!(back.params.needs_max_byte_array_len().tag(), "absent");
        assert_eq!(back.params.service_shape().tag(), "absent");
        // And none of them yields a value by any accessor.
        assert!(back.params.declared().stated().is_none());
        assert!(back.params.service_shape().stated().is_none());
    }

    /// Zero declared parameters is a DEMAND, not an absence (D7). An image that
    /// declares `params:` on every node and names none of them has said its
    /// store holds only the seeded `use_sim_time`.
    #[test]
    fn zero_declared_parameters_is_a_demand_and_survives_unfloored() {
        let mut d = SizingDescriptor::new("p", Status::Derived, Basis::Contract);
        d.params
            .set_declared(Some(0))
            .set_max_parameters(Some(1))
            .set_max_param_name_len(Some(12));
        let back = parse(&render(&d), Path::new("p.toml")).unwrap();
        assert_eq!(back.params.declared().stated(), Some(&0));
        assert_eq!(back.params.max_parameters().stated(), Some(&1));
    }

    // --- negative controls: a reader that would default silently -------------

    #[test]
    fn a_corrupt_value_refuses_rather_than_defaulting() {
        let text = render(&island()).replace("depth = 10", "depth = \"ten\"");
        let err = parse(&text, Path::new("t.toml")).unwrap_err();
        assert!(!err.is_missing());
        let msg = err.to_string();
        assert!(msg.contains("does not parse"), "{msg}");
    }

    #[test]
    fn an_unknown_key_refuses_rather_than_being_ignored() {
        let text = format!("{}\nmax_kinds_v2 = 3\n", render(&island()));
        let err = parse(&text, Path::new("t.toml")).unwrap_err();
        assert!(err.to_string().contains("max_kinds_v2"), "{err}");
    }

    #[test]
    fn an_unknown_vocabulary_spelling_refuses() {
        let text = render(&island()).replace("\"keep_last\"", "\"keep_some\"");
        let err = parse(&text, Path::new("t.toml")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("keep_some"), "{msg}");
        assert!(msg.contains("keep_last"), "{msg}");
    }

    #[test]
    fn a_field_both_stated_and_refused_is_a_contradiction() {
        // The shape that reads as derived to a consumer checking only the value,
        // and as refused to one checking only the table. It must not parse.
        let text = format!(
            "{}\n[target.refused]\npointer_bytes = \"unknown triple\"\n",
            render(&island())
        );
        let err = parse(&text, Path::new("t.toml")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("both states and refuses"), "{msg}");
        assert!(msg.contains("pointer_bytes"), "{msg}");
    }

    #[test]
    fn a_refusal_naming_no_field_is_an_error() {
        // A refusal nobody can read is worse than none: the consumer defaults
        // silently and the producer believes it warned.
        let mut d = island();
        d.target = Target::new(None, None, None);
        let text = format!(
            "{}\n[target.refused]\npointer_wdith = \"typo\"\n",
            render(&d)
        );
        let err = parse(&text, Path::new("t.toml")).unwrap_err();
        assert!(err.to_string().contains("pointer_wdith"), "{err}");
    }

    #[test]
    fn a_schema_version_this_reader_does_not_know_refuses() {
        let text = render(&island()).replace(
            &format!("schema_version = {SCHEMA_VERSION}"),
            &format!("schema_version = {}", SCHEMA_VERSION + 1),
        );
        let err = parse(&text, Path::new("t.toml")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("schema_version"), "{msg}");
        assert!(msg.contains("nros sync"), "{msg}");
    }

    #[test]
    fn a_missing_file_is_distinguishable_from_a_broken_one() {
        let dir = tempfile::tempdir().unwrap();
        let p = descriptor_path(&dir.path().join("build"), "nobody");
        let err = read(&p).unwrap_err();
        assert!(err.is_missing(), "{err}");

        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "schema_version = ").unwrap();
        let err = read(&p).unwrap_err();
        assert!(!err.is_missing(), "{err}");
    }

    /// The watch names the path, not the file that may be at it.
    ///
    /// A descriptor that does not exist yet is the state a build is in BEFORE
    /// the first `nros sync`, and the sync that creates one has to be able to
    /// re-run this unit. Measured on cargo 1.98.1: with the watch emitted only
    /// for a file that already existed, neither the creation nor a later edit
    /// re-ran the script — see `load_for_build_script`'s table.
    #[test]
    fn a_descriptor_that_does_not_exist_yet_is_watched_anyway() {
        let dir = tempfile::tempdir().unwrap();
        let p = descriptor_path(&dir.path().join("build"), "nobody");
        let mut lines: Vec<String> = Vec::new();
        let err = load_for_build_script_emitting(&p, &mut |l| lines.push(l.to_string()))
            .expect_err("nothing is there");

        assert!(err.is_missing(), "{err}");
        assert_eq!(
            lines,
            [format!("cargo::rerun-if-changed={}", p.display())],
            "the creation edge is the whole reason this is a file"
        );
    }

    /// And exactly once for one that IS there — a second line would be
    /// harmless to cargo and a sign the two roads had been written twice.
    #[test]
    fn a_descriptor_that_exists_is_watched_once() {
        let dir = tempfile::tempdir().unwrap();
        let p = descriptor_path(&dir.path().join("build"), "talker");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, render(&island())).unwrap();

        let mut lines: Vec<String> = Vec::new();
        let desc = load_for_build_script_emitting(&p, &mut |l| lines.push(l.to_string())).unwrap();

        assert_eq!(desc.meta.entry, "talker");
        assert_eq!(lines, [format!("cargo::rerun-if-changed={}", p.display())]);
    }

    #[test]
    fn the_descriptor_path_is_one_rule() {
        assert_eq!(
            descriptor_path(Path::new("ws/build"), "talker"),
            Path::new("ws/build/nros/sizing/talker.toml")
        );
    }

    #[test]
    fn a_checkout_path_in_a_refusal_reason_is_a_portability_violation() {
        // Issue 0320. The way this goes wrong is a producer interpolating
        // `path.display()` into prose, and the rendered text is where it is
        // cheapest to notice.
        let checkout = tempfile::tempdir().unwrap();
        let mut d = island();
        d.types.refuse(
            "distinct_count",
            format!(
                "no bound inventory at {}",
                checkout.path().join("generated").display()
            ),
        );
        let hit = portability_violation(&render(&d), &[checkout.path()])
            .expect("the leaked checkout path was caught");
        assert!(hit.contains("0320"), "{hit}");
        assert!(hit.contains(checkout.path().to_str().unwrap()), "{hit}");
    }

    #[test]
    fn a_topic_is_not_mistaken_for_a_path() {
        // `/localization/kinematic_state` is an ordinary ROS topic and appears
        // both as an identity key and inside refusal prose. A guard keyed on
        // "starts with a slash" would fire on it, which is why this one is keyed
        // on the CHECKOUT ROOTS the producer names instead -- a guard that cries
        // wolf on a legal image is worse than no guard.
        let mut d = SizingDescriptor::new("island", Status::Partial, Basis::Contract);
        let mut ep = Endpoint::new(
            EndpointKind::Subscription,
            "nav_msgs/msg/Odometry",
            "/localization/kinematic_state",
        );
        ep.refuse(
            "depth",
            "history = keep_all on subscription /localization/kinematic_state",
        );
        d.endpoints.push(ep);
        let text = render(&d);
        let back = parse(&text, Path::new("i.toml")).unwrap();
        assert_eq!(back.endpoints[0].topic, "/localization/kinematic_state");
        let checkout = tempfile::tempdir().unwrap();
        assert!(portability_violation(&text, &[checkout.path()]).is_none());
    }

    #[test]
    fn a_root_that_cannot_discriminate_is_ignored_rather_than_matched() {
        // `/` is in every descriptor that carries a topic. A guard that fired on
        // it would fire on every call, which is indistinguishable from no guard
        // -- and worse, because the next person would delete it as noise.
        let text = render(&island());
        assert!(portability_violation(&text, &[Path::new("/")]).is_none());
        assert!(portability_violation(&text, &[Path::new("relative/dir")]).is_none());
    }
}
