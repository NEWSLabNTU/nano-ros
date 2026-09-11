//! `buffer:` — the fault-analysis discipline, two diagnostics, and the one
//! derivation it selects. **phase-454 W8, RFC-0100 D9.**
//!
//! # What `buffer:` is, and what it is not
//!
//! `buffer: latest | queue` has been in the contract schema since Vocabulary
//! v2, on `EndpointProps::buffer` in the pinned `ros-launch-manifest`:
//!
//! > *"buffering discipline for a `state: true` subscription. `latest`
//! > (default) — staleness is the failure mode; `queue` — backlog is the
//! > failure mode, drained batch-wise by the consuming timer. Only meaningful
//! > alongside `state: true`; parse-time error otherwise."*
//!
//! That is FAULT-ANALYSIS vocabulary. An earlier draft of RFC-0100 proposed it
//! as the `SLOTS` source and D9 retracts that, because it is deterministic in
//! neither direction: `queue` states no depth, `latest` does not forbid a depth
//! above one, and the key is only defined for `state: true` endpoints, which
//! are a minority. A sizing input that answers "some number" for one value and
//! "not necessarily one" for the other is not a sizing input.
//!
//! So it earns its keep twice over instead.
//!
//! # 1. Two diagnostics ([`BufferDiagnostic`])
//!
//! Each is one author statement contradicting another one beside it, and
//! NEITHER IS AN ERROR — a legitimate image can want either, and this module
//! refuses to decide that for it. They name the endpoint and both values.
//!
//! # 2. The derivation ([`derive_queue_depth`])
//!
//! `queue` is *"drained batch-wise by the consuming timer"*, so the depth such
//! an endpoint needs is a function of how fast it fills and how fast it drains,
//! and the contract already carries both rates. This is a DEFAULT under the
//! RFC-0049 ladder — a stated `depth` still wins — for exactly the endpoints
//! where a hand-typed depth is most likely to be wrong.
//!
//! # What the SystemModel actually carries today (MEASURED, issue 1339)
//!
//! The three facts this module needs are authored in the contract and reach
//! nano-ros unevenly, and the measurement is the reason this wave lands armed
//! rather than firing. Resolved with the pinned `nros-launch-resolve` over a
//! contract stating all three (`tests/fixtures/queue_buffer/`):
//!
//! | fact | contract key | in the SystemModel? |
//! | --- | --- | --- |
//! | publish rate | `topics.<t>.rate_hz` | **yes** — `TopicContract::rate_hz` |
//! | publish rate | `<node>.pub.<ep>.min_rate_hz` | **yes** — `PubContract::min_rate_hz` |
//! | drain rate | `<node>.paths.<p>.trigger.timer.rate_hz` | **NO** — `PathContract` has no trigger |
//! | discipline | `<node>.sub.<ep>.buffer` | **NO** — `SubContract` has no `buffer` |
//!
//! Both missing halves are dropped by the MODEL SCHEMA, not by nano-ros: the
//! resolver reads them, reasons about them, and emits neither. It emits a
//! diagnostic proving it computed exactly the division below —
//!
//! > `[queue-drain-rate] warning: node 'listener' timer path 'drain' rate_hz
//! > (10) is less than the sum of its 'buffer: queue' subscriptions' producer
//! > rates (50, from ["chatter"]) — the queue will accumulate backlog every
//! > period`
//!
//! — and then writes a `node_paths` entry carrying `output` alone. That is
//! issue 1256's shape one layer upstream of where W3 found it: a declaration
//! legal to write, legal to resolve, and dropped before any consumer can read
//! it. Issue 1339 carries it; `contract_queue_buffer_reaches_the_model.rs` is
//! the tripwire that goes red the day it closes.
//!
//! Which is why nothing here guesses. An endpoint whose discipline or whose
//! rates did not arrive gets NO DEFAULT and a reason saying which one is
//! missing ([`NoDefault`]), because RFC-0100 D6's rule holds here as everywhere
//! else: *"a refused fact never silently widens its basis."*

use std::fmt;

// ---------------------------------------------------------------------------
// The vocabulary. ONE spelling, for `qos_override`'s reason.
// ---------------------------------------------------------------------------
//
// `nros_orchestration_ir::qos_override` holds the four QoS policy vocabularies
// because "whatever the macro and the CLI must agree on lives here", and the
// four copies of the decoder that preceded it had drifted. `buffer` is NOT one
// of those: it never reaches a `qos_overrides.*` parameter, it is not a QoS
// policy, and no runtime folds it into a `QoSProfile`. It is a contract-side
// analysis fact with exactly one reader, which is this file — so it lives here,
// beside the derivation it selects, rather than in a module about QoS.

/// The buffering discipline a `state: true` subscription declared.
///
/// `None` on an [`crate::entity_inventory::EntityDecl`] means NOBODY SAID, and
/// it must never read as `Latest`: `latest` IS the schema's default, but "the
/// author wrote `latest`" and "the author wrote nothing" license different
/// actions here. The first is a statement this module diagnoses against a large
/// depth; the second is silence, which it says nothing about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferDiscipline {
    /// Read-latest; STALENESS is the failure mode.
    Latest,
    /// Bounded queue drained batch-wise by the consuming timer; BACKLOG is the
    /// failure mode.
    Queue,
}

/// The accepted `buffer` spellings, for a diagnostic.
pub const BUFFER_VALUES: &str = "`latest` or `queue`";

/// Parse a `buffer` value. `None` for a spelling the schema does not model —
/// an ERROR for every caller, never a skip.
pub fn parse_buffer(value: &str) -> Option<BufferDiscipline> {
    match value.trim() {
        "latest" => Some(BufferDiscipline::Latest),
        "queue" => Some(BufferDiscipline::Queue),
        _ => None,
    }
}

/// The canonical spelling, for rendering one back out.
pub fn buffer_spelling(b: BufferDiscipline) -> &'static str {
    match b {
        BufferDiscipline::Latest => "latest",
        BufferDiscipline::Queue => "queue",
    }
}

// ---------------------------------------------------------------------------
// Rates, as exact integers.
// ---------------------------------------------------------------------------

/// A rate in MILLIHERTZ.
///
/// The contract authors a rate as a decimal and the model carries it as `f64`,
/// and this type is the boundary where that stops. Two reasons, and the second
/// is the load-bearing one:
///
/// * [`crate::entity_inventory::EntityDecl`] derives `Eq`, which an `f64` field
///   would forbid — and a derived `PartialEq` over floats is a hazard in its own
///   right, since two rates that differ in the last bit would make two otherwise
///   identical inventories unequal and two `NaN`s make one unequal to itself.
/// * The derivation is a CEILING DIVISION. In integers it is exact;
///   `(p / d).ceil()` in floating point is not — `(0.3f64 / 0.1).ceil()` is 3
///   on some inputs and 4 on others, and a depth that depends on which is a
///   size that depends on the decimal spelling of a rate.
///
/// Millihertz rather than hertz because a contract may legitimately author a
/// sub-hertz rate (a 0.5 Hz health beat), and rounding that to an integer hertz
/// would be a divide-by-zero. Three decimal places is where the schema's own
/// examples stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RateMilliHz(u64);

impl RateMilliHz {
    /// Build one from a contract's `f64`. `None` for anything that is not a
    /// rate: a negative, a zero, a `NaN`, an infinity, or a value so small it
    /// rounds to zero millihertz.
    ///
    /// Zero is refused rather than carried, for [`EntityDecl::depth`]'s reason
    /// one field over: a zero rate is a typo for "I did not want to say", and
    /// the two license opposite actions — a rate DIVIDES, and a zero drain rate
    /// would either panic or produce a depth of infinity.
    ///
    /// [`EntityDecl::depth`]: crate::entity_inventory::EntityDecl::depth
    pub fn from_hz(hz: f64) -> Option<Self> {
        if !hz.is_finite() || hz <= 0.0 {
            return None;
        }
        let milli = (hz * 1000.0).round();
        if milli < 1.0 || milli > u64::MAX as f64 {
            return None;
        }
        Some(Self(milli as u64))
    }

    /// The rate in millihertz.
    pub fn milli_hz(self) -> u64 {
        self.0
    }

    /// The rate in hertz, for a message a human reads.
    pub fn hz(self) -> f64 {
        self.0 as f64 / 1000.0
    }
}

impl fmt::Display for RateMilliHz {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Trailing zeroes trimmed so `50` prints as `50 Hz`, not `50.000 Hz`,
        // while `0.5` keeps the digit that distinguishes it from `0`.
        let hz = self.hz();
        if (hz.fract()).abs() < f64::EPSILON {
            write!(f, "{} Hz", hz as u64)
        } else {
            write!(f, "{hz} Hz")
        }
    }
}

// ---------------------------------------------------------------------------
// The derivation.
// ---------------------------------------------------------------------------

/// The margin added to the arrivals-per-drain-period count. **One slot.**
///
/// # Why there is a margin at all
///
/// `ceil(publish / drain)` is the number of samples the producer emits during
/// one drain period, and it is the right answer only for a queue whose drain
/// instants are ALIGNED with its arrivals. They are not: a `queue` subscription
/// and the timer that drains it are two independent periodic streams with no
/// phase relationship the contract states. For an unaligned window of length
/// `T`, the number of arrivals of a `p`-periodic stream is bounded by
/// `floor(p·T) + 1`, not `p·T` — a sample that arrives just after one drain
/// instant waits a full period, and is still in the queue when the next
/// period's own samples land. One slot is exactly that straggler.
///
/// # Why it is not two, and not a percentage
///
/// Because every extra slot is a whole message. The arena charges
/// `(depth + 1) * bound + (depth + 1) * pointer` per subscription, so a margin
/// is priced in bytes at the image's largest type, and inflating a DEFAULT is
/// the over-size direction this RFC keeps measuring (the island arena, 207,096
/// bytes against 71,664, is the same mistake made by a different default).
///
/// And because the things a bigger margin would absorb — timer jitter, drain
/// overrun, a burst above the declared rate — are bounded by NO number in the
/// contract that reaches here. A margin chosen to cover them would be a guess
/// wearing arithmetic's clothes, which is precisely what D9 refuses to let
/// `buffer:` itself be. The contract does carry `paths.<p>.max_jitter` and
/// `paths.<p>.miss`; when those reach the model (issue 1339 is the same gap),
/// a jitter-aware margin can be DERIVED here and this constant retired. Until
/// then the honest margin is the one the phase relationship alone forces.
pub const QUEUE_DEPTH_MARGIN: u32 = 1;

/// The depth at which a `buffer: latest` endpoint stops being read-latest.
///
/// Not a taste threshold: it is the arena's own break point. The descriptor
/// writer prices `depth <= 1` as a triple buffer (`TripleBuffer::SLOT_COUNT`)
/// and everything above it as `(depth + 1)` full slots, so depth 2 is the first
/// depth at which an endpoint that declared staleness as its failure mode
/// starts paying for history by the message.
pub const LATEST_DEPTH_DIAGNOSTIC_FLOOR: u32 = 2;

/// Why an endpoint got no derived default.
///
/// Every variant is a REASON A USER CAN ACT ON, not a silent skip. This is the
/// whole of acceptance 3: where either rate is absent there is no default, and
/// the absence is visible rather than filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoDefault {
    /// The endpoint declared no `buffer:` at all, or declared `latest`. Only a
    /// `queue` endpoint is drained batch-wise, and the derivation is a
    /// statement about draining.
    NotAQueue,
    /// A depth was STATED. The ladder ends here and nothing below it runs —
    /// this is not a failure, it is the rung above working.
    DepthStated(u32),
    /// No publish rate reached this endpoint.
    NoPublishRate,
    /// No drain rate reached this endpoint.
    NoDrainRate,
}

impl NoDefault {
    /// The prose a consumer shows. Names the contract key that would supply the
    /// missing fact, because a refusal a user cannot act on is a refusal they
    /// work around.
    pub fn reason(&self) -> String {
        match self {
            NoDefault::NotAQueue => "the endpoint declares no `buffer: queue`, and only a queue \
                                     is drained batch-wise by a timer -- a `latest` subscription \
                                     keeps the newest sample and has no backlog to size"
                .to_string(),
            NoDefault::DepthStated(d) => format!(
                "the endpoint states `depth: {d}`, which WINS -- RFC-0049's ladder puts a \
                 stated value above every derived one, and this derivation only fills in \
                 where nobody stated"
            ),
            NoDefault::NoPublishRate => "no publish rate reached this endpoint. State \
                                         `topics.<topic>.rate_hz`, or `min_rate_hz` on the \
                                         publishing endpoint; without it the number of samples \
                                         arriving in a drain period is unknown and a guessed \
                                         depth is a guessed buffer size"
                .to_string(),
            NoDefault::NoDrainRate => "no drain rate reached this endpoint. It is authored as \
                                       `paths.<path>.trigger: { timer: { rate_hz: N } }` on the \
                                       consuming node, and the SystemModel does not carry a \
                                       path's trigger today (issue 1339) -- the resolver reads \
                                       it, warns on it, and emits only the path's `output`. \
                                       Until it travels, state `min_rate_hz` on what that timer \
                                       publishes, which is the rate this reader can see"
                .to_string(),
        }
    }
}

/// The arithmetic, over nothing but two rates.
///
/// ```text
/// depth = ceil(publish_rate / drain_rate) + QUEUE_DEPTH_MARGIN
/// ```
///
/// Exact: both rates are integer millihertz, so the ceiling division is integer
/// division and carries no floating-point tie to break. See
/// [`QUEUE_DEPTH_MARGIN`] for the margin's argument.
///
/// A queue drained FASTER than it fills still gets `1 + margin = 2`: the
/// ceiling never falls below one, because at least one sample arrives per
/// period the producer is live, and the straggler slot is a property of the
/// phase relationship rather than of the ratio.
///
/// Saturates at [`u32::MAX`] rather than wrapping, which is unreachable for any
/// rate pair a contract can state (it would need a ratio above four billion)
/// and is spelled anyway so that the one arithmetic in this module has no
/// panicking path at all.
pub fn derive_queue_depth(publish: RateMilliHz, drain: RateMilliHz) -> u32 {
    let p = publish.milli_hz();
    let d = drain.milli_hz();
    // Both are non-zero by construction -- `RateMilliHz::from_hz` refuses a
    // zero -- so this division cannot trap.
    let per_period = p.div_ceil(d);
    let depth = per_period.saturating_add(QUEUE_DEPTH_MARGIN as u64);
    u32::try_from(depth).unwrap_or(u32::MAX)
}

/// Resolve one endpoint's depth default, ladder and all.
///
/// The ORDER is the ladder, and it is asserted rather than implied: a stated
/// depth is checked FIRST, so no combination of rates can ever displace one.
pub fn depth_default(
    buffer: Option<BufferDiscipline>,
    stated_depth: Option<u32>,
    publish: Option<RateMilliHz>,
    drain: Option<RateMilliHz>,
) -> Result<u32, NoDefault> {
    if let Some(d) = stated_depth {
        return Err(NoDefault::DepthStated(d));
    }
    if buffer != Some(BufferDiscipline::Queue) {
        return Err(NoDefault::NotAQueue);
    }
    let publish = publish.ok_or(NoDefault::NoPublishRate)?;
    let drain = drain.ok_or(NoDefault::NoDrainRate)?;
    Ok(derive_queue_depth(publish, drain))
}

// ---------------------------------------------------------------------------
// The diagnostics.
// ---------------------------------------------------------------------------

/// One author statement contradicting another one beside it.
///
/// **A warning, never an error.** Both shapes are legal and a legitimate image
/// can want either: a `queue` at depth 1 is a deliberate drop-oldest-immediately
/// channel, and a `latest` at depth 10 may be a subscription whose QoS has to
/// match a publisher it does not own. This module reports what the author wrote
/// on both lines and lets them decide; making either fatal would be this
/// module deciding an application question from a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferDiagnostic {
    /// `buffer: queue` with `depth: 1` — backlog declared as the failure mode,
    /// then sized for no backlog at all.
    QueueSizedForNoBacklog {
        kind: &'static str,
        topic: String,
        depth: u32,
    },
    /// `buffer: latest` with a depth at or above
    /// [`LATEST_DEPTH_DIAGNOSTIC_FLOOR`] — paying by the message for history
    /// the author declared they do not read.
    LatestPaysForHistory {
        kind: &'static str,
        topic: String,
        depth: u32,
    },
}

impl BufferDiagnostic {
    /// The prose. Names the endpoint and BOTH values, because the point of
    /// either diagnostic is that two lines of one endpoint disagree and the
    /// author has to see both to pick which one to change.
    pub fn message(&self) -> String {
        match self {
            BufferDiagnostic::QueueSizedForNoBacklog { kind, topic, depth } => format!(
                "{kind} `{topic}` declares `buffer: queue` -- backlog is its failure mode -- \
                 and `depth: {depth}` beside it, which holds no backlog. A KEEP_LAST(1) queue \
                 drops every sample but the newest between drains, which is `buffer: latest` \
                 behaviour under a `queue` declaration. Not an error: a deliberate \
                 drop-oldest channel is a legitimate design. Raise the depth, or say \
                 `buffer: latest` and mean it."
            ),
            BufferDiagnostic::LatestPaysForHistory { kind, topic, depth } => format!(
                "{kind} `{topic}` declares `buffer: latest` -- staleness is its failure mode, \
                 so it reads only the newest sample -- and `depth: {depth}` beside it. Every \
                 slot above one is a whole message of receive region the endpoint has \
                 declared it will not read. Not an error: a depth can be there to match a \
                 publisher this node does not own. Drop it to 1, or say `buffer: queue` if \
                 the backlog is real."
            ),
        }
    }
}

/// Diagnose one endpoint. `None` when the two lines agree, or when either is
/// missing — silence is not a contradiction.
pub fn diagnose(
    kind: &'static str,
    topic: &str,
    buffer: Option<BufferDiscipline>,
    depth: Option<u32>,
) -> Option<BufferDiagnostic> {
    let (buffer, depth) = (buffer?, depth?);
    match buffer {
        BufferDiscipline::Queue if depth <= 1 => Some(BufferDiagnostic::QueueSizedForNoBacklog {
            kind,
            topic: topic.to_string(),
            depth,
        }),
        BufferDiscipline::Latest if depth >= LATEST_DEPTH_DIAGNOSTIC_FLOOR => {
            Some(BufferDiagnostic::LatestPaysForHistory {
                kind,
                topic: topic.to_string(),
                depth,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hz(v: f64) -> RateMilliHz {
        RateMilliHz::from_hz(v).expect("a positive finite rate")
    }

    /// ACCEPTANCE 1, the arithmetic itself: `ceil(pub / drain) + 1`.
    ///
    /// The table carries the three cases that are structurally different, not
    /// three samples of one: an exact multiple (where the ceiling is a no-op
    /// and the margin is the ONLY thing separating the answer from a naive
    /// division), a non-multiple (where the ceiling is what stops a truncation
    /// from under-sizing), and a queue drained faster than it fills.
    #[test]
    fn the_derived_depth_is_arrivals_per_drain_period_plus_one() {
        // Exact multiple: 50 Hz in, 10 Hz out -> 5 arrivals a period, +1.
        assert_eq!(derive_queue_depth(hz(50.0), hz(10.0)), 6);
        // Not a multiple: 50/12 = 4.16.. -> the CEILING is 5, +1. A truncating
        // division would say 4+1 = 5 and under-size by a slot every period.
        assert_eq!(derive_queue_depth(hz(50.0), hz(12.0)), 6);
        assert_eq!(derive_queue_depth(hz(100.0), hz(30.0)), 5);
        // Drained faster than filled: one arrival, plus the straggler.
        assert_eq!(derive_queue_depth(hz(1.0), hz(10.0)), 2);
        // Sub-hertz, which is why the unit is millihertz: 0.5 Hz in, 0.1 Hz
        // out is 5 a period. Rounded to integer hertz the drain rate would be
        // zero and this would divide by it.
        assert_eq!(derive_queue_depth(hz(0.5), hz(0.1)), 6);
    }

    /// The ceiling is EXACT, which floating point would not be.
    ///
    /// `0.3 / 0.1` is `2.9999999999999996` in IEEE 754 and its ceiling is 3;
    /// the true ratio is 3 and the true answer is 3 + 1. A float implementation
    /// gets this one right by luck and `0.7 / 0.1` (`6.999...` -> 7) wrong in
    /// the other direction. Integer millihertz has no such cases, and this test
    /// is the one that fails if someone moves the arithmetic back to `f64`.
    #[test]
    fn the_ceiling_does_not_depend_on_a_floating_point_tie() {
        assert_eq!(derive_queue_depth(hz(0.3), hz(0.1)), 4);
        assert_eq!(derive_queue_depth(hz(0.7), hz(0.1)), 8);
        assert_eq!(derive_queue_depth(hz(0.1), hz(0.3)), 2);
    }

    /// ACCEPTANCE 2: a stated depth beats the derived default, and it does so
    /// FIRST -- before the discipline and before either rate is looked at.
    ///
    /// Asserted with rates that WOULD derive a different number, because a
    /// ladder that only holds when the lower rung has nothing to say is not a
    /// ladder.
    #[test]
    fn a_stated_depth_beats_the_derived_default() {
        let derived = derive_queue_depth(hz(50.0), hz(10.0));
        assert_eq!(derived, 6, "the rung below would have said 6");
        assert_eq!(
            depth_default(
                Some(BufferDiscipline::Queue),
                Some(3),
                Some(hz(50.0)),
                Some(hz(10.0))
            ),
            Err(NoDefault::DepthStated(3)),
            "a stated 3 must win over a derived 6"
        );
        // And with no depth stated, the same endpoint DOES get the 6 -- the
        // control that keeps the assertion above from passing vacuously.
        assert_eq!(
            depth_default(
                Some(BufferDiscipline::Queue),
                None,
                Some(hz(50.0)),
                Some(hz(10.0))
            ),
            Ok(6)
        );
    }

    /// ACCEPTANCE 3: either rate absent means NO default, and the reason names
    /// the contract key that would supply it.
    #[test]
    fn a_missing_rate_yields_no_default_and_a_reason_that_names_the_key() {
        let no_pub = depth_default(Some(BufferDiscipline::Queue), None, None, Some(hz(10.0)))
            .expect_err("no publish rate means no default");
        assert_eq!(no_pub, NoDefault::NoPublishRate);
        assert!(
            no_pub.reason().contains("rate_hz") && no_pub.reason().contains("min_rate_hz"),
            "the reason must name the keys that would fix it: {}",
            no_pub.reason()
        );

        let no_drain = depth_default(Some(BufferDiscipline::Queue), None, Some(hz(50.0)), None)
            .expect_err("no drain rate means no default");
        assert_eq!(no_drain, NoDefault::NoDrainRate);
        assert!(
            no_drain.reason().contains("trigger") && no_drain.reason().contains("1339"),
            "the reason must name the key AND the issue that keeps it from arriving: {}",
            no_drain.reason()
        );

        // Neither rate: the PUBLISH one is reported, because it is the fact
        // that is actually reachable today and therefore the one an author can
        // act on first.
        assert_eq!(
            depth_default(Some(BufferDiscipline::Queue), None, None, None),
            Err(NoDefault::NoPublishRate)
        );
    }

    /// A `latest` endpoint -- and an endpoint that stated no discipline at all
    /// -- gets no default however complete its rates are. This is the half of
    /// the no-op proof that lives at the arithmetic.
    #[test]
    fn only_a_queue_endpoint_is_ever_defaulted() {
        for buffer in [None, Some(BufferDiscipline::Latest)] {
            assert_eq!(
                depth_default(buffer, None, Some(hz(50.0)), Some(hz(10.0))),
                Err(NoDefault::NotAQueue),
                "{buffer:?} must not be sized from a drain rate"
            );
        }
    }

    /// ACCEPTANCE 4, first shape: `queue` at depth 1.
    #[test]
    fn a_queue_sized_for_no_backlog_is_diagnosed() {
        let d = diagnose(
            "subscription",
            "/image",
            Some(BufferDiscipline::Queue),
            Some(1),
        )
        .expect("queue at depth 1 is the diagnosed shape");
        let m = d.message();
        assert!(m.contains("/image"), "{m}");
        assert!(m.contains("queue") && m.contains("depth: 1"), "{m}");
        assert!(
            m.contains("Not an error"),
            "the diagnostic must say it is not fatal: {m}"
        );
        // Depth 2 is the first depth that holds a backlog, so it is silent.
        assert_eq!(
            diagnose(
                "subscription",
                "/image",
                Some(BufferDiscipline::Queue),
                Some(2)
            ),
            None
        );
    }

    /// ACCEPTANCE 4, second shape: `latest` at a depth above the read-latest
    /// break point.
    #[test]
    fn a_latest_paying_for_history_is_diagnosed() {
        let d = diagnose(
            "subscription",
            "/odom",
            Some(BufferDiscipline::Latest),
            Some(10),
        )
        .expect("latest at depth 10 is the diagnosed shape");
        let m = d.message();
        assert!(m.contains("/odom") && m.contains("depth: 10"), "{m}");
        assert!(m.contains("Not an error"), "{m}");
        // Depth 1 IS read-latest, so it agrees with the declaration.
        assert_eq!(
            diagnose(
                "subscription",
                "/odom",
                Some(BufferDiscipline::Latest),
                Some(1)
            ),
            None
        );
    }

    /// Silence is not a contradiction. An endpoint missing either line is not
    /// diagnosed -- which is what keeps every image that states no `buffer:`
    /// (all of them today) out of the warning stream entirely.
    #[test]
    fn a_missing_line_is_never_a_contradiction() {
        assert_eq!(diagnose("subscription", "/t", None, Some(10)), None);
        assert_eq!(
            diagnose("subscription", "/t", Some(BufferDiscipline::Queue), None),
            None
        );
        assert_eq!(diagnose("subscription", "/t", None, None), None);
    }

    /// The vocabulary round-trips, and a spelling the schema does not model is
    /// `None` rather than a silently defaulted `latest`.
    ///
    /// `latest` IS the schema's default when the key is absent, which is
    /// exactly why a MISSPELLING must not resolve to it: an author who writes
    /// `buffer: qeue` would otherwise get read-latest semantics and a green
    /// build, having declared the opposite.
    #[test]
    fn the_buffer_vocabulary_round_trips_and_refuses_everything_else() {
        for (s, b) in [
            ("latest", BufferDiscipline::Latest),
            ("queue", BufferDiscipline::Queue),
        ] {
            assert_eq!(parse_buffer(s), Some(b), "{s}");
            assert_eq!(buffer_spelling(b), s);
        }
        for bad in ["", "Queue", "QUEUE", "qeue", "ring", "keep_all", "latest "] {
            if bad.trim() == "latest" {
                continue;
            }
            assert_eq!(parse_buffer(bad), None, "{bad:?} must not parse");
        }
    }

    /// A rate that is not a rate does not become one.
    #[test]
    fn a_zero_or_nonfinite_rate_is_not_a_rate() {
        for bad in [0.0, -1.0, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(RateMilliHz::from_hz(bad), None, "{bad}");
        }
        // Below one millihertz rounds to zero, which would divide by zero.
        assert_eq!(RateMilliHz::from_hz(0.0004), None);
        assert_eq!(
            RateMilliHz::from_hz(0.001).map(RateMilliHz::milli_hz),
            Some(1)
        );
    }
}
