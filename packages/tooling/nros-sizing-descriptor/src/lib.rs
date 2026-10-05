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
///   names the row, and a consumer sizes from
///   [`transient_local_publishers_bound_over`] instead -- the worst case,
///   never zero (issue 1572).
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

/// Issue 1655 — the largest QoS depth any SUBSCRIPTION declares, guarded on
/// every one of them declaring.
///
/// The rule `NROS_DECLARED_MAX_QOS_DEPTH` carried, given ONE spelling here so
/// a consumer reads the descriptor rather than restating the reduction:
///
/// * [`Fact::Stated`] — the maximum, when EVERY subscription row states its
///   depth. A table over the annotated subset would size the image from part
///   of itself, which is why one silent row refuses the whole answer.
/// * [`Fact::Refused`] — some subscription row's depth is refused or absent;
///   the prose names it. The consumer keeps its builtin (ROS's KEEP_LAST(10)),
///   which is the safe direction for a ring depth.
/// * [`Fact::Absent`] — no subscription rows at all: nothing to read.
///
/// A row refused by D12 rule 2 (two entries' models disagreeing on one
/// subscription's depth) refuses here too, for the same reason: neither value
/// is the runtime's.
pub fn max_subscription_depth(desc: &SizingDescriptor) -> Fact<usize> {
    let mut max: Option<usize> = None;
    for e in desc
        .endpoints
        .iter()
        .filter(|e| e.kind == EndpointKind::Subscription)
    {
        match e.depth() {
            Fact::Stated(d) => {
                let d = d as usize;
                max = Some(max.map_or(d, |m| m.max(d)));
            }
            f => {
                return Fact::Refused(format!(
                    "subscription {} ({}) states no depth{}, so the image's largest depth \
                     is not known",
                    e.topic,
                    e.type_name,
                    f.refusal().map(|r| format!(": {r}")).unwrap_or_default()
                ));
            }
        }
    }
    match max {
        Some(m) => Fact::Stated(m),
        None => Fact::Absent,
    }
}

/// One row of the [`transient_local_publishers`] rule.
///
/// Issue 1378 — the rule needed a SECOND caller, and the descriptor is not it.
/// A cmake / Zephyr / NuttX entry had no sizing descriptor at all when 1378 was
/// filed (a Zephyr west entry named none to cargo until issue 1407, a
/// multi-entry configure until RFC-0100 D12), so the only rows such a road
/// could offer were its DECLARED ENTITIES. Without a row shape
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

/// The TRANSIENT_LOCAL publishers a POOL must be sized for -- the count when
/// [`transient_local_publishers_over`] states one, and its WORST CASE when it
/// refuses.
///
/// Issue 1572. A refusal used to contribute ZERO to both pools this count
/// sizes, which is the unsafe direction: the queryable table and the retention
/// pool then held no slot for the latched publisher the refusal was about, and
/// the first `create_publisher` for it failed with `Full` at boot (code -3 on
/// the C++ ABI). Measured on the Autoware Safety Island before issue 1567
/// removed the silent row that triggered it.
///
/// RFC-0100 D6 already says what a refusal falls back to: *"worst case when
/// refused, always the safe direction and always loud"* (XRCE assumes reliable
/// and pays both buffers). The worst case here is that every publisher whose
/// durability nobody stated IS transient-local, so this counts
///
/// * a `publisher` row unless it states `volatile`, and
/// * every `action_server` row, as the rule does.
///
/// That is a BOUND: the true count can only be lower, by exactly the silent
/// rows that turn out volatile. A pool sized from it cannot fall short, and
/// the cost of the over-count is one queryable-table entry and one retention
/// slot per silent row -- which stating the row's durability gives back.
///
/// Why a bound and not a refused CONFIGURE: the bound breaks no image that
/// boots today, and a refusal would fail the build of every image with a
/// silent volatile publisher, which is most of them. The refusal prose is
/// still carried, by [`transient_local_publishers_over`], so the consumer
/// can say what declaring would save.
///
/// `None` iff the rule answers [`Fact::Absent`] (no rows: nobody described the
/// image, and the consumer keeps its builtin).
pub fn transient_local_publishers_bound_over<'a, I>(rows: I) -> Option<usize>
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
                Fact::Stated(Durability::Volatile) => {}
                _ => count += 1,
            },
            _ => {}
        }
    }
    any.then_some(count)
}

/// [`transient_local_publishers_bound_over`] over a descriptor's endpoint rows.
pub fn transient_local_publishers_bound(desc: &SizingDescriptor) -> Option<usize> {
    transient_local_publishers_bound_over(desc.endpoints.iter().map(|e| TlRow {
        kind: e.kind,
        durability: e.durability(),
        topic: &e.topic,
        type_name: &e.type_name,
    }))
}

/// The bytes ONE transient-local retention slot must hold, from a descriptor's
/// `[[endpoint]]` rows -- the LEAF road's answer to the question
/// `_nros_bounds_tl_retain` (`cmake/NanoRosMessageBounds.cmake`) answers on the
/// cmake roads (issue 1498).
///
/// `nros-rmw-zenoh` keeps one slot per transient-local publisher, each
/// `ZPICO_TL_RETAIN_BYTES` long, and the slot was a flat 1024 B on every road
/// that is not the Zephyr resolver: measured on the Autoware Safety Island,
/// five latched publishers of 13-105 B each held 5 x 1024 B.
///
/// The slot is the largest `wire_bound_bytes` over the publisher rows that state
/// `transient_local`. That is the type's RECEIVE bound (`BoundState::rx`, the
/// larger of the XCDR1/XCDR2 encodings, transport-framed), which is never below
/// what this stack serializes (`tx`), so it can only over-size the slot -- the
/// safe direction, by at most the framing and the XCDR2 header.
///
/// The SAME refusals as the cmake function, so the roads cannot disagree about
/// WHEN an answer exists:
///
/// * the count is not [`Fact::Stated`] (no rows, or a publisher states no
///   durability) -- the refusal is carried, or [`Fact::Absent`] passes through;
/// * the count is zero -- a pool of no slots has no slot size, so
///   [`Fact::Absent`] and the consumer keeps its builtin;
/// * an `action_server` row is present -- its `/status` publisher is
///   transient-local by protocol and its type appears in no row, so a maximum
///   over the rows would miss it;
/// * any transient-local publisher row has no stated `wire_bound_bytes`.
///
/// Not floored (D7): a consumer that needs a minimum applies its own.
pub fn transient_local_retain_bytes(desc: &SizingDescriptor) -> Fact<usize> {
    match transient_local_publishers(desc) {
        Fact::Stated(0) | Fact::Absent => return Fact::Absent,
        Fact::Refused(r) => return Fact::Refused(r),
        Fact::Stated(_) => {}
    }
    if let Some(a) = desc
        .endpoints
        .iter()
        .find(|e| e.kind == EndpointKind::ActionServer)
    {
        return Fact::Refused(format!(
            "action server {} ({}) declares a transient-local `/status` publisher whose \
             type no endpoint row names, so no maximum over the rows bounds its slot",
            a.topic, a.type_name
        ));
    }
    let mut max = 0usize;
    for e in desc.endpoints.iter().filter(|e| {
        e.kind == EndpointKind::Publisher
            && matches!(e.durability(), Fact::Stated(Durability::TransientLocal))
    }) {
        match e.wire_bound_bytes() {
            Fact::Stated(b) => max = max.max(b),
            f => {
                return Fact::Refused(format!(
                    "publisher {} ({}) is transient-local and states no `wire_bound_bytes`: {}",
                    e.topic,
                    e.type_name,
                    f.refusal().unwrap_or("nothing derived it"),
                ));
            }
        }
    }
    Fact::Stated(max)
}

/// The zenoh subscriber PAYLOAD CLASSES an image's subscriptions need — issue
/// 1595, RFC-0100 D5 ("each backend's build reads the descriptor and computes
/// its own knobs").
///
/// Three numbers that move together, for the reason the leaf road's module
/// gives (`nros_cli_core::leaf_payload_classes`): the runtime routes on
/// `min(threshold, small block)`, so publishing the large COUNT without the
/// small BLOCK classifies a type here one way and routes it another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PayloadClasses {
    /// Subscribing ENTITIES whose `rx` bound is over the ceiling. Entities,
    /// not types: two subscriptions on one large type need two blocks. Zero is
    /// an answer ("every type fits the small class"), never an abstention.
    pub large_count: usize,
    /// The largest `rx` among those, or 0 when there are none.
    pub large_max: usize,
    /// The largest `rx` at or under the ceiling, or 0 when nothing fits under it.
    pub small_max: usize,
}

impl PayloadClasses {
    /// Classify ONE subscribing entity by its `rx` bound.
    ///
    /// THE rule — `rx > ceiling` is large — and the only spelling of it in Rust:
    /// the cargo-leaf road's join and `nros-rmw-zenoh`'s descriptor reader both
    /// call this, so the two cannot come to disagree about which side of the
    /// ceiling a type lands on (issue 1025). The CMake twin is
    /// `_nros_bounds_publish_payload_classes`.
    pub fn add(&mut self, rx: usize, ceiling: usize) {
        if rx > ceiling {
            self.large_count += 1;
            self.large_max = self.large_max.max(rx);
        } else {
            self.small_max = self.small_max.max(rx);
        }
    }
}

/// [`PayloadClasses`] over a descriptor's `subscription` rows, or a refusal.
///
/// REFUSES — and the caller keeps its other rung — when the rows cannot be the
/// whole subscribed set or a row has no bound:
///
/// * `[image] subscription_entities` is not stated, or does not equal the
///   number of `subscription` rows. A row the endpoint table dropped (no type,
///   no topic) is a subscription whose size nobody priced, and a class
///   derived over the rest is SHORT — `SubscriberCreationFailed` at
///   registration, the direction RFC-0100 D6 forbids.
/// * a `subscription` row's `wire_bound_bytes` is not stated.
///
/// [`Fact::Absent`] never: an image that subscribes to nothing states
/// `subscription_entities = 0` and gets the all-zero classes, which is the
/// answer (no large block at all).
pub fn subscriber_payload_classes(desc: &SizingDescriptor, ceiling: usize) -> Fact<PayloadClasses> {
    let subs: Vec<&Endpoint> = desc
        .endpoints
        .iter()
        .filter(|e| e.kind == EndpointKind::Subscription)
        .collect();
    match desc.image.subscription_entities() {
        Fact::Stated(n) if n == subs.len() => {}
        Fact::Stated(n) => {
            return Fact::Refused(format!(
                "the image creates {n} subscription(s) and the endpoint table describes {}, so \
                 a payload class derived over the rows would leave the rest unpriced",
                subs.len()
            ));
        }
        f => {
            return Fact::Refused(format!(
                "`[image] subscription_entities` is not stated ({}), so nothing says the \
                 subscription rows are every subscription this image creates",
                f.refusal().unwrap_or("absent")
            ));
        }
    }
    let mut classes = PayloadClasses::default();
    for s in subs {
        match s.wire_bound_bytes() {
            Fact::Stated(rx) => classes.add(rx, ceiling),
            f => {
                return Fact::Refused(format!(
                    "subscription {} ({}) states no `wire_bound_bytes`: {}",
                    s.topic,
                    s.type_name,
                    f.refusal().unwrap_or("nothing derived it")
                ));
            }
        }
    }
    Fact::Stated(classes)
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
///
/// **2** (RFC-0100 Amendment 1, issues 1649 + 1595 — ONE bump for both, as the
/// amendment requires): `[meta] composed_entries` (D12, the runtime a
/// multi-entry configure shares) and `[types] max_wire_bound_bytes` (the
/// closure fact `RX_BUF` is sized from). Both are new KEYS, and the reader
/// refuses unknown keys, so a version-1 reader handed either would fail on the
/// key rather than on the version — the bump makes that failure say why.
pub const SCHEMA_VERSION: u32 = 2;

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

/// RFC-0100 D12 (issue 1649) — `<build_dir>/nros/sizing/runtime/shared.toml`,
/// the descriptor for the ONE runtime a multi-entry configure links into every
/// entry.
///
/// A sub-DIRECTORY rather than a reserved stem beside the entries: an entry is
/// a CMake target name, and `[A-Za-z0-9_.+-]` can spell any stem this could
/// pick, while no target name contains a `/`. So the runtime's file can never
/// be an entry's, and an entry named `runtime` keeps `runtime.toml`.
pub fn runtime_descriptor_path(build_dir: &Path) -> PathBuf {
    build_dir
        .join(SIZING_SUBDIR)
        .join("runtime")
        .join("shared.toml")
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
    // Issue 1623 — the VARIABLE is an input too, not only the file it names.
    // Without this edge a build script that first ran with the variable UNSET
    // (a sizes probe or metadata pass sharing the target dir) is never re-run
    // when a later build sets it: cargo re-runs on exactly the inputs a script
    // declares, and `rerun-if-changed` on the path is only emitted when there
    // IS a path. Every consumer then kept its undeclared defaults while the
    // descriptor sat beside the build stating every count.
    from_env_value_emitting(std::env::var(DESCRIPTOR_ENV).ok(), &mut |line| {
        println!("{line}")
    })
}

/// [`from_build_env`] over a given variable value, with the cargo directives
/// handed to `emit` — split out so a test can assert the VARIABLE is watched
/// on every road, unset included (issue 1623).
fn from_env_value_emitting(
    value: Option<String>,
    emit: &mut dyn FnMut(&str),
) -> Result<Option<SizingDescriptor>, DescriptorError> {
    emit(&format!("cargo::rerun-if-env-changed={DESCRIPTOR_ENV}"));
    match value {
        Some(v) if !v.trim().is_empty() => {
            load_for_build_script_emitting(Path::new(v.trim()), emit).map(Some)
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 1595 -- the payload classes over a descriptor's subscription rows.
    fn subs_desc(bounds: &[Option<usize>], entities: Option<usize>) -> SizingDescriptor {
        let mut d = SizingDescriptor::new("e", Status::Derived, Basis::Contract);
        for (i, b) in bounds.iter().enumerate() {
            let mut s = Endpoint::new(EndpointKind::Subscription, "p/msg/T", format!("/t{i}"));
            s.set_wire_bound_bytes(*b);
            d.endpoints.push(s);
        }
        d.image.set_subscription_entities(entities);
        d
    }

    #[test]
    fn payload_classes_split_entities_at_the_ceiling() {
        let d = subs_desc(&[Some(12), Some(1500), Some(4096), Some(4096)], Some(4));
        assert_eq!(
            subscriber_payload_classes(&d, 2048),
            Fact::Stated(PayloadClasses {
                large_count: 2,
                large_max: 4096,
                small_max: 1500,
            }),
            "two subscriptions on one large type need two blocks"
        );
        // The ceiling is the caller's: the same rows under a 1024 ceiling.
        assert_eq!(
            subscriber_payload_classes(&d, 1024)
                .stated()
                .map(|c| c.large_count),
            Some(3)
        );
        // Nothing subscribed is an ANSWER: no large block, no small size.
        assert_eq!(
            subscriber_payload_classes(&subs_desc(&[], Some(0)), 2048),
            Fact::Stated(PayloadClasses::default())
        );
    }

    /// The two refusals -- each is an under-size if it were not one.
    #[test]
    fn payload_classes_refuse_an_incomplete_or_unpriced_row_set() {
        // A subscription the table does not describe.
        assert!(
            subscriber_payload_classes(&subs_desc(&[Some(12)], Some(2)), 2048)
                .refusal()
                .is_some()
        );
        // Nothing says the rows are complete.
        assert!(
            subscriber_payload_classes(&subs_desc(&[Some(12)], None), 2048)
                .refusal()
                .is_some()
        );
        // A row with no bound.
        assert!(
            subscriber_payload_classes(&subs_desc(&[Some(12), None], Some(2)), 2048)
                .refusal()
                .is_some_and(|r| r.contains("/t1"))
        );
    }

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

    /// Issue 1655 -- the guarded MAX the `NROS_DECLARED_MAX_QOS_DEPTH` carrier
    /// delivered, given one spelling: every subscription states a depth, or no
    /// answer.
    #[test]
    fn max_subscription_depth_is_the_guarded_max_over_subscriptions() {
        let mut d = island();
        assert_eq!(max_subscription_depth(&d), Fact::Stated(10));

        let mut one = Endpoint::new(EndpointKind::Subscription, "std_msgs/msg/Int32", "/n");
        one.set_depth(Some(1));
        d.endpoints.push(one);
        assert_eq!(max_subscription_depth(&d), Fact::Stated(10), "a max");

        // A PUBLISHER's depth is not a receive ring's: it does not count.
        let mut p = Endpoint::new(EndpointKind::Publisher, "std_msgs/msg/Int32", "/p");
        p.set_depth(Some(50));
        d.endpoints.push(p);
        assert_eq!(max_subscription_depth(&d), Fact::Stated(10));

        // One silent subscription refuses the WHOLE answer -- a max over the
        // rows that answered is not a bound on the one that did not.
        d.endpoints.push(Endpoint::new(
            EndpointKind::Subscription,
            "std_msgs/msg/String",
            "/silent",
        ));
        assert!(
            max_subscription_depth(&d)
                .refusal()
                .is_some_and(|r| r.contains("/silent"))
        );

        // No subscription at all is not "depth 0": there is nothing to read.
        let empty = SizingDescriptor::new("e", Status::Derived, Basis::Contract);
        assert_eq!(max_subscription_depth(&empty), Fact::Absent);
    }

    /// Issue 1498 -- the LEAF road's retention slot: the largest bound over the
    /// transient-local publisher rows, and the cmake road's refusals.
    #[test]
    fn transient_local_retain_bytes_is_the_largest_tl_publisher_bound() {
        let mut d = island();
        // No transient-local publisher: no slot to size.
        assert_eq!(transient_local_retain_bytes(&d), Fact::Absent);

        let mut small = Endpoint::new(EndpointKind::Publisher, "pkg/msg/Small", "/a");
        small
            .set_durability(Some(Durability::TransientLocal))
            .set_wire_bound_bytes(Some(16));
        d.endpoints.push(small);
        let mut big = Endpoint::new(EndpointKind::Publisher, "pkg/msg/Big", "/b");
        big.set_durability(Some(Durability::TransientLocal))
            .set_wire_bound_bytes(Some(112));
        d.endpoints.push(big);
        // A volatile publisher's bound is not a retention slot's business,
        // however large -- and neither is the 1170 B subscription.
        let mut vol = Endpoint::new(EndpointKind::Publisher, "pkg/msg/Huge", "/c");
        vol.set_durability(Some(Durability::Volatile))
            .set_wire_bound_bytes(Some(4096));
        d.endpoints.push(vol);
        assert_eq!(transient_local_retain_bytes(&d), Fact::Stated(112));

        // An unpriced transient-local row refuses, naming itself.
        let mut open = Endpoint::new(EndpointKind::Publisher, "pkg/msg/Open", "/open");
        open.set_durability(Some(Durability::TransientLocal));
        let mut with_open = d.clone();
        with_open.endpoints.push(open);
        match transient_local_retain_bytes(&with_open) {
            Fact::Refused(r) => assert!(r.contains("/open"), "{r}"),
            other => panic!("expected a refusal, got {other:?}"),
        }

        // An action server's `/status` type is in no row: refused.
        d.endpoints.push(Endpoint::new(
            EndpointKind::ActionServer,
            "example_interfaces/action/Fibonacci",
            "/fibonacci",
        ));
        assert!(matches!(
            transient_local_retain_bytes(&d),
            Fact::Refused(r) if r.contains("/fibonacci")
        ));
    }

    /// A refused COUNT refuses the slot size too: the silent publisher may be
    /// latched, and its type was never looked at.
    #[test]
    fn a_refused_tl_count_refuses_the_retain_bytes() {
        let mut d = island();
        let mut tl = Endpoint::new(EndpointKind::Publisher, "pkg/msg/Small", "/a");
        tl.set_durability(Some(Durability::TransientLocal))
            .set_wire_bound_bytes(Some(16));
        d.endpoints.push(tl);
        d.endpoints.push(Endpoint::new(
            EndpointKind::Publisher,
            "pkg/msg/Silent",
            "/silent",
        ));
        assert!(matches!(
            transient_local_retain_bytes(&d),
            Fact::Refused(r) if r.contains("/silent")
        ));
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

    /// Issue 1572 -- a refused count still sizes its pools, from the WORST
    /// case: every publisher whose durability nobody stated counts as
    /// transient-local. Before this, a refusal contributed zero, and the
    /// latched publisher the refusal was about had no queryable slot at boot.
    #[test]
    fn a_refused_count_sizes_its_pools_from_the_worst_case() {
        let mut d = island();
        let mut tl = Endpoint::new(EndpointKind::Publisher, "std_msgs/msg/String", "/latched");
        tl.set_durability(Some(Durability::TransientLocal));
        d.endpoints.push(tl);
        let mut vol = Endpoint::new(EndpointKind::Publisher, "std_msgs/msg/String", "/chatter");
        vol.set_durability(Some(Durability::Volatile));
        d.endpoints.push(vol);
        // Stated: the bound IS the count.
        assert_eq!(transient_local_publishers(&d), Fact::Stated(1));
        assert_eq!(transient_local_publishers_bound(&d), Some(1));

        d.endpoints.push(Endpoint::new(
            EndpointKind::Publisher,
            "std_msgs/msg/String",
            "/unstated",
        ));
        d.endpoints.push(Endpoint::new(
            EndpointKind::ActionServer,
            "example_interfaces/action/Fibonacci",
            "/fibonacci",
        ));
        assert!(matches!(transient_local_publishers(&d), Fact::Refused(_)));
        assert_eq!(
            transient_local_publishers_bound(&d),
            Some(3),
            "latched + the silent publisher + the action server's /status; \
             the volatile row costs nothing"
        );
        assert_eq!(
            transient_local_publishers_bound(&SizingDescriptor::new(
                "bare",
                Status::Derived,
                Basis::Contract
            )),
            None,
            "no rows is still Absent, not a bound of zero"
        );
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

    /// `[params]` is purely ADDITIVE (issue 1408 did not bump the version), so
    /// a descriptor with no such section parses. Every accessor then answers
    /// `Absent`, which is the true answer — the file's writer knew nothing
    /// about parameters.
    #[test]
    fn a_descriptor_with_no_params_section_parses_and_reads_absent() {
        let text = "\
schema_version = 2

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

    /// RFC-0100 Amendment 1 — the ONE bump D12 and issue 1595 share. A
    /// version-1 file is refused by the version, naming the remedy, rather
    /// than read with two keys it could not have had.
    #[test]
    fn a_version_one_descriptor_is_refused_after_the_d12_bump() {
        assert_eq!(SCHEMA_VERSION, 2);
        let text = render(&island()).replace("schema_version = 2", "schema_version = 1");
        let err = parse(&text, Path::new("t.toml")).unwrap_err();
        assert!(err.to_string().contains("nros sync"), "{err}");
    }

    /// D12 rule 4 + issue 1595 — both new keys round-trip, and an entry's own
    /// descriptor (no composed entries) carries no `composed_entries` line, so
    /// it reads exactly as before apart from the version.
    #[test]
    fn the_runtime_identity_and_the_closure_bound_round_trip() {
        let mut d = island();
        d.meta
            .set_composed_entries(vec!["b_entry".into(), "a_entry".into(), "b_entry".into()]);
        d.types.set_max_wire_bound_bytes(Some(1496));
        let text = render(&d);
        assert!(
            text.contains("composed_entries = [\"a_entry\", \"b_entry\"]"),
            "sorted and de-duplicated: {text}"
        );
        let back = parse(&text, Path::new("shared.toml")).unwrap();
        assert_eq!(back.meta.composed_entries(), ["a_entry", "b_entry"]);
        assert_eq!(back.types.max_wire_bound_bytes().stated(), Some(&1496));
        assert_eq!(back, {
            let mut want = d.clone();
            want.meta
                .set_composed_entries(vec!["a_entry".into(), "b_entry".into()]);
            want
        });

        let mut plain = island();
        plain
            .types
            .refuse("max_wire_bound_bytes", "a/msg/Open is unbounded");
        let text = render(&plain);
        assert!(!text.contains("composed_entries"), "{text}");
        let back = parse(&text, Path::new("p.toml")).unwrap();
        assert!(back.meta.composed_entries().is_empty());
        assert_eq!(
            back.types.max_wire_bound_bytes().refusal(),
            Some("a/msg/Open is unbounded")
        );
    }

    /// The runtime's file can never be an entry's: an entry is a CMake target
    /// name and none contains a `/`.
    #[test]
    fn the_runtime_path_is_disjoint_from_every_entry_path() {
        let b = Path::new("/b");
        let rt = runtime_descriptor_path(b);
        assert_eq!(rt, Path::new("/b/nros/sizing/runtime/shared.toml"));
        assert_ne!(rt, descriptor_path(b, "runtime"));
        assert_ne!(rt, descriptor_path(b, "shared"));
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
    /// Issue 1623 — the VARIABLE is watched whether or not it is set. A build
    /// script that first ran with it unset and declared no edge on it was never
    /// re-run when a later build set it, so every consumer of the descriptor
    /// kept its undeclared defaults (measured: a declared C++ talker built at
    /// the 74,240-byte worst-case arena while its descriptor stated one timer).
    #[test]
    fn the_descriptor_variable_is_watched_even_when_unset() {
        for value in [None, Some(String::new())] {
            let mut lines: Vec<String> = Vec::new();
            let got =
                from_env_value_emitting(value.clone(), &mut |l| lines.push(l.to_string())).unwrap();
            assert!(got.is_none());
            assert_eq!(
                lines,
                [format!("cargo::rerun-if-env-changed={DESCRIPTOR_ENV}")],
                "{value:?}"
            );
        }
        let dir = tempfile::tempdir().unwrap();
        let p = descriptor_path(&dir.path().join("build"), "talker");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, render(&island())).unwrap();
        let mut lines: Vec<String> = Vec::new();
        let got = from_env_value_emitting(Some(p.display().to_string()), &mut |l| {
            lines.push(l.to_string())
        })
        .unwrap();
        assert!(got.is_some());
        assert_eq!(
            lines,
            [
                format!("cargo::rerun-if-env-changed={DESCRIPTOR_ENV}"),
                format!("cargo::rerun-if-changed={}", p.display()),
            ]
        );
    }

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
