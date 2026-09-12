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
pub use schema::{Endpoint, Image, Meta, Policy, SizingDescriptor, Target, Types};
pub use vocabulary::{
    Basis, Durability, EndpointKind, History, RegistrationPath, Reliability, Status,
};

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
/// whole point of a file transport — and does so only when the file EXISTS,
/// because a trigger on an absent path leaves the unit permanently dirty (issue
/// 0490 and `scripts/check-build-rs-rerun-paths.py`). A path that was named and
/// is not there is an error rather than a silent skip: somebody pointed the build
/// at a descriptor, and "it wasn't there so I used my defaults" is the shape D6
/// exists to forbid.
pub fn load_for_build_script(path: &Path) -> Result<SizingDescriptor, DescriptorError> {
    if !path.exists() {
        return Err(DescriptorError::Missing(path.to_path_buf()));
    }
    println!("cargo::rerun-if-changed={}", path.display());
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
            .set_registration_path(Some(RegistrationPath::RustTypedSchemaless))
            .set_storage_bytes(Some(280))
            .set_wire_bound_bytes(Some(1170));
        d.endpoints.push(sub);
        d.types = Types::new(Some(7), Some(12), Some(48), Some(3));
        d.policy = Policy::new(Some(64), Some(4096), Some(1));
        d.image = Image::new(Some(2), Some(1), Some(3));
        d
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
