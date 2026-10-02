//! phase-454 W10 — the DECLARED QoS history depth, and what Rust does with it.
//!
//! # What this is
//!
//! The QoS history DEPTH of a subscription multiplies the executor arena:
//! `sizeof(entry) + (depth + 1) * bound + (depth + 1) * 8`. A build that sizes
//! from a declared depth and an image that registers a different one are an
//! image sized for one number and running another — so the two have to be held
//! to one. C++ holds them with a `static_assert` inside `NROS_SUBSCRIBE`, and C
//! with a `_Static_assert` inside `NROS_ASSERT_DECLARED_DEPTH`.
//!
//! # Why Rust's half is at REGISTRATION and not at compile time
//!
//! Both C surfaces get a compile-time answer because a preprocessor macro sees
//! the call site's TOPIC as a token. Rust has neither of the two things that
//! would make the same answer possible:
//!
//!   * **No macro seam at the subscribe site.** `nros::main!` emits the boot
//!     scaffold and knows nothing about subscriptions; there is no
//!     `nros::subscribe!`. Every call site is an ordinary method call.
//!   * **The topic is a runtime `&str`**, not a const generic. A `const`
//!     assertion would need it in the type system.
//!
//! And the descriptor road differs too: a RUST component is registered as an
//! empty cmake INTERFACE library (`NanoRosNodeRegister.cmake`), compiled by the
//! workspace runtime crate rather than by that target, so there is no C
//! preprocessor in the lane and no include path to put a header on. That is why
//! `_nros_declared_qos_arm()` returns early for `RUST` — the header mechanism
//! is not available to it, not merely unimplemented.
//!
//! # TWO facts arrive, on two roads
//!
//! The declared depths reach `nros-node/build.rs` from whichever carrier the
//! road has, and it writes both into [`crate::config::DECLARED_QOS_ROWS`]:
//!
//!   * the **cmake road** (native C/C++, Zephyr west, NuttX) sets
//!     `NROS_ENTITY_DECLARED_DEPTHS` (`type|topic=depth`), written by the entity
//!     inventory from the resolved SystemModel;
//!   * the **single-package cargo leaf road** — the one phase-454 W12 taught to
//!     read a `system.contract.yaml` — sets no such variable and names a SIZING
//!     DESCRIPTOR instead, whose `[[endpoint]]` rows carry the same fact
//!     (RFC-0100 D4). W10 shipped reading only the first, so on exactly the road
//!     that had a contract this table was `None` and nothing could be compared.
//!
//! # And two things are done with them, because the surfaces differ
//!
//!   * [`check`](crate::declared_qos::check) REFUSES a disagreement, with
//!     [`NodeError::DeclaredDepthMismatch`] — the registration fails, the
//!     subscription never exists, and the entry's `?` carries it out. Reached
//!     from the C and C++ FFI seams, which took the declared depth at their own
//!     call site already.
//!   * [`honour`](crate::declared_qos::honour) is what a RUST registration calls: it TAKES a declared depth
//!     shallower than the ask (the C++ three-argument `NROS_SUBSCRIBE`'s answer,
//!     delivered one layer down because Rust has no call-site seam) and refuses
//!     a declaration DEEPER than the ask. Its doc comment has the whole rule and
//!     why the asymmetry is real.
//!
//! # ABSENCE IS NOT ZERO
//!
//! A `(type, topic)` with no row is "nobody declared this endpoint". Nothing is
//! checked for it, nothing is taken, and nothing is defaulted. An image with no
//! contract sidecar has `DECLARED_QOS_ROWS == None` and behaves exactly as it
//! did before — measured, not asserted: on `examples/native/rust/listener` with
//! the sidecar removed, `.bss + .data` and every attributed symbol are
//! identical to `origin/main`'s, and the image carries no `declared_qos` symbol
//! at all.
//!
//! # issue 1256 -- RELIABILITY and DURABILITY too
//!
//! A contract can state three things about a subscription and this module now
//! reads all three. The two policies are not a sizing fact the way depth is;
//! they are an INTEROP one -- an incompatible-QoS match never delivers -- and
//! until this they were declared, delivered to the build and never compared
//! with the code. They arrive on the sizing DESCRIPTOR only (no env knob is
//! added for them: re-carrying a fact the descriptor already states is the
//! 0460/0491 shape), and [`check`](crate::declared_qos::check) / [`honour`](crate::declared_qos::honour) treat them by the same rules as
//! the depth, with the order of the two values standing in for "deeper".
//!
//! # issue 1608 -- PUBLISHERS too, with the policy direction REVERSED
//!
//! A contract can state the same three things about a publisher, and
//! `transient_local` is the one QoS fact that is publisher-side by nature: it is
//! what a zenoh image spends a queryable slot on. The descriptor's publisher
//! rows reach [`DECLARED_PUBLISHER_QOS_ROWS`](crate::declared_qos::DECLARED_PUBLISHER_QOS_ROWS)
//! (a table of its own: a publisher and a subscription on one `(type, topic)`
//! are two endpoints), and every publisher registration -- the Rust seams and
//! the C/C++ FFI seams alike -- goes through
//! [`honour_publisher`](crate::declared_qos::honour_publisher).
//!
//! Its rule is NOT the subscription's copied across. Reliability and durability
//! are matched request-vs-offered, so the tolerant direction flips: a reader may
//! ask for less than a writer offers, a writer may offer more than a reader asks.
//! A publisher asking LESS than its declaration is therefore RAISED to it (the
//! build reserved it, and every reader that matched the lower offer still
//! matches); one asking MORE is REFUSED (lowering it could break a match, and
//! keeping it rides storage the build did not count). The depth rule is shared.
//! Measured on `examples/native/rust/talker` with a contract declaring
//! `/chatter` `transient_local`, depth 1: the default publisher registers
//! transient_local, depth 1, and reports both takes; declaring `best_effort`
//! instead refuses the registration and the image exits naming the topic.

use crate::executor::NodeError;
use nros_rmw::{QoSDurabilityPolicy, QoSProfile, QoSReliabilityPolicy};

/// One subscription this image's contract declares something about.
///
/// Each column is `None` where nobody stated it -- a row exists when ANY of the
/// three is stated, so any one of them may be silent. `type_name` appears once
/// in the ROS spelling and once DDS-mangled, as two rows, because which one a
/// generated message class carries is a property of the codegen that produced
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclaredEndpoint {
    pub type_name: &'static str,
    pub topic: &'static str,
    /// The declared KEEP_LAST depth.
    pub depth: Option<u32>,
    /// `Reliable` / `BestEffort` -- never `SystemDefault`, which says "the
    /// middleware chooses" and so cannot be disagreed with.
    pub reliability: Option<QoSReliabilityPolicy>,
    /// `Volatile` / `TransientLocal`, on the same rule.
    pub durability: Option<QoSDurabilityPolicy>,
}

/// Every subscription this image's contract declares something about, or
/// `None` when it declares nothing.
pub use crate::config::DECLARED_QOS_ROWS;

/// issue 1608 -- every PUBLISHER this image's contract declares something
/// about, or `None`. Same row shape as [`DECLARED_QOS_ROWS`]; read by
/// [`honour_publisher`].
pub use crate::config::DECLARED_PUBLISHER_QOS_ROWS;

/// Do two topic spellings name the same endpoint?
///
/// One leading `/` is ignored on each side: a contract states `/chatter` and a
/// call site may write either `"/chatter"` or the relative `"chatter"` (
/// `examples/workspaces/launch/src/listener_pkg` writes the second). Nothing
/// deeper is normalised — a namespace is a different endpoint, and guessing at
/// one here would make the check match rows nobody meant.
fn topic_eq(a: &str, b: &str) -> bool {
    let a = a.strip_prefix('/').unwrap_or(a);
    let b = b.strip_prefix('/').unwrap_or(b);
    a == b
}

/// The declared row for `(type_name, topic)` in `rows`, or `None`.
///
/// Split out so the lookup is testable against a table this build did not
/// generate: in-tree, `DECLARED_QOS_ROWS` is `None` (no contract reaches a
/// unit-test build), so a test that could only call the public form would
/// assert nothing while passing.
fn row_in<'r>(
    rows: &'r [DeclaredEndpoint],
    type_name: &str,
    topic: &str,
) -> Option<&'r DeclaredEndpoint> {
    rows.iter()
        .find(|r| r.type_name == type_name && topic_eq(r.topic, topic))
}

/// The declared depth for `(type_name, topic)` in `rows`, or `None`.
fn depth_in(rows: &[DeclaredEndpoint], type_name: &str, topic: &str) -> Option<u32> {
    row_in(rows, type_name, topic).and_then(|r| r.depth)
}

/// The declared depth for `(type_name, topic)`, or `None` when nobody declared
/// that endpoint's depth.
///
/// Linear over a handful of rows, on a path that runs once per subscription at
/// registration. `None` when this image declares nothing at all.
pub fn declared_depth(type_name: &str, topic: &str) -> Option<u32> {
    depth_in(DECLARED_QOS_ROWS?, type_name, topic)
}

/// issue 1256 -- the declared reliability for `(type_name, topic)`, or `None`.
pub fn declared_reliability(type_name: &str, topic: &str) -> Option<QoSReliabilityPolicy> {
    row_in(DECLARED_QOS_ROWS?, type_name, topic).and_then(|r| r.reliability)
}

/// issue 1256 -- the declared durability for `(type_name, topic)`, or `None`.
pub fn declared_durability(type_name: &str, topic: &str) -> Option<QoSDurabilityPolicy> {
    row_in(DECLARED_QOS_ROWS?, type_name, topic).and_then(|r| r.durability)
}

/// The contract's own spelling of a reliability, for a log line.
fn reliability_spelling(r: QoSReliabilityPolicy) -> &'static str {
    match r {
        QoSReliabilityPolicy::Reliable => "reliable",
        QoSReliabilityPolicy::BestEffort => "best_effort",
        QoSReliabilityPolicy::SystemDefault => "system_default",
        QoSReliabilityPolicy::Unknown => "unknown",
    }
}

/// The contract's own spelling of a durability, for a log line.
fn durability_spelling(d: QoSDurabilityPolicy) -> &'static str {
    match d {
        QoSDurabilityPolicy::Volatile => "volatile",
        QoSDurabilityPolicy::TransientLocal => "transient_local",
        QoSDurabilityPolicy::SystemDefault => "system_default",
        QoSDurabilityPolicy::Unknown => "unknown",
    }
}

/// How much a reliability ASKS FOR: the order [`honour`] reads "stronger" in.
/// `None` is "states no opinion" -- `SystemDefault` hands the choice to the
/// middleware and `Unknown` is a read-back sentinel nobody requests.
fn reliability_rank(r: QoSReliabilityPolicy) -> Option<u8> {
    match r {
        QoSReliabilityPolicy::BestEffort => Some(0),
        QoSReliabilityPolicy::Reliable => Some(1),
        QoSReliabilityPolicy::SystemDefault | QoSReliabilityPolicy::Unknown => None,
    }
}

/// How much a durability ASKS FOR. See [`reliability_rank`].
fn durability_rank(d: QoSDurabilityPolicy) -> Option<u8> {
    match d {
        QoSDurabilityPolicy::Volatile => Some(0),
        QoSDurabilityPolicy::TransientLocal => Some(1),
        QoSDurabilityPolicy::SystemDefault | QoSDurabilityPolicy::Unknown => None,
    }
}

/// Refuse a subscription whose QoS disagrees with what the contract declared
/// for its topic -- its depth, and (issue 1256) its reliability and durability.
///
/// `Ok(())` when every declared column agrees AND when nothing was declared --
/// an image that has not opted in is not an image in error. On a disagreement
/// the TOPIC and BOTH values go to the log first (a `NodeError` is a unit
/// variant and can carry none of the three), then the registration is
/// refused: [`NodeError::DeclaredDepthMismatch`] for the depth,
/// [`NodeError::DeclaredQosMismatch`] for a policy.
///
/// Reached from the **C and C++ FFI registration seams**, which have already
/// taken the declared QoS at their own call site (`NROS_SUBSCRIBE`'s
/// three-argument form → `qos_from_declared`; C's `_Static_assert`), so a
/// disagreement arriving here is a real one. The Rust seams call [`honour`]
/// instead — see its doc comment for why the two surfaces differ.
///
/// # Keep the message inside ONE log line
///
/// `nros_log`'s formatting buffer is 256 bytes by default (`buffer-size-256`)
/// and OVERFLOW TRUNCATES with a single `…`. This message used to run past 450
/// characters, so on a real image the half a reader needs — which file to edit —
/// was the half that got cut, and the diagnostic ended mid-sentence. The
/// numbers and the topic come first for the same reason: a long type name may
/// still push the tail off, and the tail is the least load-bearing part.
pub fn check(type_name: &str, topic: &str, qos: &QoSProfile) -> Result<(), NodeError> {
    match DECLARED_QOS_ROWS {
        Some(rows) => check_in(rows, type_name, topic, qos),
        None => Ok(()),
    }
}

/// [`check`] against a table this build did not generate. See [`row_in`].
fn check_in(
    rows: &[DeclaredEndpoint],
    type_name: &str,
    topic: &str,
    qos: &QoSProfile,
) -> Result<(), NodeError> {
    let Some(row) = row_in(rows, type_name, topic) else {
        return Ok(());
    };
    if let Some(declared) = row.depth
        && declared != qos.depth
    {
        nros_log::log_error!(
            nros_log::get_logger("nros_node"),
            "declared depth: `{}` registers KEEP_LAST({}) but this image's contract \
             declares {}, and the arena was sized from {}. Fix the contract row or the \
             QoS at the call site (type `{}`).",
            topic,
            qos.depth,
            declared,
            declared,
            type_name
        );
        return Err(NodeError::DeclaredDepthMismatch);
    }
    if let Some(declared) = row.reliability
        && declared != qos.reliability
    {
        nros_log::log_error!(
            nros_log::get_logger("nros_node"),
            "declared QoS: `{}` registers {} but this image's contract declares reliability \
             {}; an incompatible match never delivers. Fix the contract row or the call site.",
            topic,
            reliability_spelling(qos.reliability),
            reliability_spelling(declared)
        );
        return Err(NodeError::DeclaredQosMismatch);
    }
    if let Some(declared) = row.durability
        && declared != qos.durability
    {
        nros_log::log_error!(
            nros_log::get_logger("nros_node"),
            "declared QoS: `{}` registers {} but this image's contract declares durability \
             {}; an incompatible match never delivers. Fix the contract row or the call site.",
            topic,
            durability_spelling(qos.durability),
            durability_spelling(declared)
        );
        return Err(NodeError::DeclaredQosMismatch);
    }
    Ok(())
}

/// The QoS a Rust subscription actually registers with, once the contract has
/// had its say — phase-454 W13, the Rust half of W10, widened to the two
/// policies by issue 1256.
///
/// # Why Rust TAKES the declared value where C and C++ ASSERT it
///
/// C++ settles this at the call site, by arity: `NROS_SUBSCRIBE(Msg, m, topic)`
/// states no QoS, so `qos_from_declared` BUILDS one from the declaration
/// ("nothing to assert — there is only ever one statement"), and the
/// four-argument form, where the call site did state one, `static_assert`s that
/// the two agree. C does the same with `NROS_ASSERT_DECLARED_*`.
///
/// Rust has neither half of that seam: there is no `nros::subscribe!`, and the
/// topic is a runtime `&str`, so nothing at the call site can be told apart
/// from anything else at compile time — and by the time a QoS reaches a
/// registration, "the caller wrote this" and "a constructor defaulted it" are
/// the same `QoSProfile`. `Node::create_subscription` hands
/// `QoSProfile::default()` — `rmw_qos_profile_default`, KEEP_LAST(10),
/// RELIABLE, VOLATILE — to the same `_with_qos` entry point a caller with an
/// opinion reaches, and the declarative road folds both into one
/// `EntityMetadata::qos` field long before this point.
///
/// So the distinction C++ makes is not available here, and the two candidate
/// rules that remain each fail on their own:
///
/// * **Check only.** Every `QoSProfile::default()` call site in the tree would
///   be refused the moment its endpoint is declared at anything but the
///   default — which is the entire point of declaring one. A declaration would
///   become a way to stop an image booting.
/// * **Take, always.** A caller who narrowed a queue on purpose
///   (`create_subscription_viewable` requires KEEP_LAST(1)) or chose
///   `best_effort` for a sensor would be silently widened by a contract that
///   over-declared.
///
/// # The rule, and why the asymmetry is real
///
/// **The contract states what the BUILD RESERVED.** W12 made that literal for
/// the depth: the executor arena, the zenoh receive ring and both payload pools
/// on a declared image are all sized from the declared depth. The policies are
/// the same kind of statement: XRCE drops both reliable stream buffers to the
/// protocol floor when every endpoint declares `best_effort` (RFC-0100 D3), and
/// a `transient_local` endpoint is what the build counts a late-joiner slot
/// for. So for each column, ordered by how much it ASKS FOR (deeper; reliable
/// over best_effort; transient_local over volatile):
///
/// * `asked > declared` — the registration wants more than was bought, and
///   asking for it would ride storage the build did not reserve. **TAKEN**, and
///   reported. For the two policies it is also the direction RxO tolerates: a
///   `best_effort` or `volatile` reader still matches a `reliable` or
///   `transient_local` writer.
/// * `asked < declared` — taking would WIDEN what the code narrowed, and the
///   image paid for something nothing wants. Two statements about one fact
///   disagreeing, with no default that explains either (RFC-0100 D8).
///   **REFUSED**, naming both.
/// * equal, or nothing declared — returned unchanged. An image with no contract
///   is byte-identical to every build before this wave.
/// * a policy asked as `SystemDefault` — the code stated no opinion, so the
///   declaration is TAKEN, silently: there is nothing to report a divergence
///   from.
///
/// # And it says so
///
/// A take is REPORTED, once per endpoint that diverges — the C++ surface's
/// value is visible in the source at the call site and Rust's is not, so a
/// registration that quietly stopped being what `lib.rs` reads would be the lie
/// this whole model exists to remove. Every take here narrows what the code
/// asked for (a shallower queue, no retransmission, no late-joiner history),
/// any of which can drop a sample, so each is `WARN`.
///
/// Called instead of [`check`] at every Rust subscription registration. The C
/// and C++ FFI seams keep [`check`] — they have already taken the declared QoS
/// at their own call site, so a disagreement reaching THEM is a real refusal
/// and not a default nobody wrote.
///
/// # `#[inline]` is load-bearing, and it was MEASURED
///
/// An image with no contract must be byte-identical to every build before this
/// wave, and "the table is empty so the work folds away" is a claim about the
/// optimiser, not a fact. It was FALSE as first written: with the table
/// threaded in as an `unwrap_or(&[])` argument, `honour` stayed out of line —
/// a real `T nros_node::declared_qos::honour` symbol, +144 bytes of text and
/// +8 of data on the no-contract listener against `origin/main`, because a
/// 40-byte `QoSProfile` moving in and out by value is past LLVM's inlining
/// budget. Matching the `Option` HERE, behind `#[inline]`, is what lets a
/// `None` table fold the call to `Ok(qos)` at every one of the fourteen
/// registration sites and leave `honour_in` unreferenced. Verified by
/// rebuilding both trees and comparing the linked binary byte for byte.
#[inline]
pub fn honour(type_name: &str, topic: &str, qos: QoSProfile) -> Result<QoSProfile, NodeError> {
    let Some(rows) = DECLARED_QOS_ROWS else {
        return Ok(qos);
    };
    honour_in(rows, type_name, topic, qos)
}

/// [`honour`] against a table this build did not generate.
///
/// Split out for the reason [`row_in`] is: in-tree `DECLARED_QOS_ROWS` is
/// `None`, so a test that could only call the public form would exercise the
/// "nothing declared" arm and assert nothing about the take or the refusal
/// while reading as coverage of both.
fn honour_in(
    rows: &[DeclaredEndpoint],
    type_name: &str,
    topic: &str,
    qos: QoSProfile,
) -> Result<QoSProfile, NodeError> {
    honour_role_in(rows, type_name, topic, qos, Role::Subscription)
}

/// Which side of a match an endpoint is on -- issue 1608.
///
/// It decides ONE thing: which direction of a POLICY disagreement
/// [`honour_role_in`] may take. Depth is not a matched policy and is treated
/// the same on both sides. Reliability and durability are request-vs-offered
/// (RxO) matched, and the tolerant direction is OPPOSITE for the two roles: a
/// reader may ask for LESS than a writer offers, a writer may offer MORE than a
/// reader asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Subscription,
    Publisher,
}

impl Role {
    fn noun(self) -> &'static str {
        match self {
            Role::Subscription => "subscription",
            Role::Publisher => "publisher",
        }
    }

    /// Why a [`PolicyVerdict::Refuse`] refuses, in the log line's words.
    fn refusal_reason(self) -> &'static str {
        match self {
            Role::Subscription => "widen what the code narrowed",
            Role::Publisher => {
                "weaken the offer below the call site's, and a reader asking for it stops matching"
            }
        }
    }
}

/// What [`honour_role_in`] does with one POLICY column that disagrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyVerdict {
    /// Take the declaration and report the divergence.
    Take,
    /// Take the declaration silently: the call site stated no opinion.
    TakeSilently,
    /// Refuse the registration.
    Refuse,
}

/// THE rule for a policy disagreement, for both roles, in rank space (`None`
/// is "no opinion" -- see [`reliability_rank`]).
///
/// * **Subscription** -- refuse when the code asked for LESS than was declared
///   (taking would widen what the code narrowed); otherwise take, which lowers
///   the request into what the build reserved and is the direction RxO
///   tolerates for a reader.
/// * **Publisher** -- refuse when the code asked for MORE than was declared.
///   Taking would WEAKEN the offer below what the code asked, so a reader that
///   requests what the code offered stops matching -- an incompatible-QoS pair
///   never delivers -- while keeping the ask rides storage the build did not
///   reserve: a `transient_local` writer is a zenoh queryable slot plus its
///   retained samples, and a `reliable` one is XRCE's reliable stream history,
///   both counted from the declaration. Otherwise take: RAISING a writer's
///   offer keeps every reader that matched the lower one, and it is what the
///   build reserved.
///
/// So the two roles share the asymmetry's REASON -- never ride storage the
/// build did not reserve, never silently break a match -- and get opposite
/// directions from it, because RxO is directional.
fn policy_verdict(role: Role, asked: Option<u8>, declared: Option<u8>) -> PolicyVerdict {
    let (Some(asked), Some(declared)) = (asked, declared) else {
        return PolicyVerdict::TakeSilently;
    };
    let refuse = match role {
        Role::Subscription => asked < declared,
        Role::Publisher => asked > declared,
    };
    if refuse {
        PolicyVerdict::Refuse
    } else {
        PolicyVerdict::Take
    }
}

/// [`honour`] / [`honour_publisher`] for either [`Role`].
fn honour_role_in(
    rows: &[DeclaredEndpoint],
    type_name: &str,
    topic: &str,
    qos: QoSProfile,
    role: Role,
) -> Result<QoSProfile, NodeError> {
    let Some(row) = row_in(rows, type_name, topic) else {
        return Ok(qos);
    };
    let mut granted = qos;
    // The DEPTH rule is the same for both roles: a subscription's depth
    // multiplies the executor arena, a publisher's is its history cache (the
    // samples a transient_local writer retains for late joiners), and in both
    // the build reserved the DECLARED number.
    if let Some(declared) = row.depth
        && declared != qos.depth
    {
        if declared > qos.depth {
            nros_log::log_error!(
                nros_log::get_logger("nros_node"),
                "declared depth: {} `{}` registers KEEP_LAST({}) and the contract declares {} \
                 -- DEEPER. Taking it would widen a queue the code narrowed; fix the contract \
                 row or the call site (type `{}`).",
                role.noun(),
                topic,
                qos.depth,
                declared,
                type_name
            );
            return Err(NodeError::DeclaredDepthMismatch);
        }
        granted.depth = declared;
        nros_log::log_warn!(
            nros_log::get_logger("nros_node"),
            "declared depth: taking KEEP_LAST({}) on {} `{}`; the call site asked {} and the \
             build reserved {}. Raise the contract row to keep more (type `{}`).",
            declared,
            role.noun(),
            topic,
            qos.depth,
            declared,
            type_name
        );
    }
    if let Some(declared) = row.reliability
        && declared != qos.reliability
    {
        match policy_verdict(
            role,
            reliability_rank(qos.reliability),
            reliability_rank(declared),
        ) {
            PolicyVerdict::Refuse => {
                nros_log::log_error!(
                    nros_log::get_logger("nros_node"),
                    "declared QoS: {} `{}` registers {} and the contract declares reliability \
                     {}; taking it would {}. Fix the contract row or the call site.",
                    role.noun(),
                    topic,
                    reliability_spelling(qos.reliability),
                    reliability_spelling(declared),
                    role.refusal_reason()
                );
                return Err(NodeError::DeclaredQosMismatch);
            }
            PolicyVerdict::Take => {
                nros_log::log_warn!(
                    nros_log::get_logger("nros_node"),
                    "declared QoS: taking reliability {} on {} `{}`; the call site asked {}. \
                     Change the contract row to keep the call site's.",
                    reliability_spelling(declared),
                    role.noun(),
                    topic,
                    reliability_spelling(qos.reliability)
                );
            }
            // The call site stated no opinion: nothing diverged, so nothing is
            // reported.
            PolicyVerdict::TakeSilently => {}
        }
        granted.reliability = declared;
    }
    if let Some(declared) = row.durability
        && declared != qos.durability
    {
        match policy_verdict(
            role,
            durability_rank(qos.durability),
            durability_rank(declared),
        ) {
            PolicyVerdict::Refuse => {
                nros_log::log_error!(
                    nros_log::get_logger("nros_node"),
                    "declared QoS: {} `{}` registers {} and the contract declares durability \
                     {}; taking it would {}. Fix the contract row or the call site.",
                    role.noun(),
                    topic,
                    durability_spelling(qos.durability),
                    durability_spelling(declared),
                    role.refusal_reason()
                );
                return Err(NodeError::DeclaredQosMismatch);
            }
            PolicyVerdict::Take => {
                nros_log::log_warn!(
                    nros_log::get_logger("nros_node"),
                    "declared QoS: taking durability {} on {} `{}`; the call site asked {}. \
                     Change the contract row to keep the call site's.",
                    durability_spelling(declared),
                    role.noun(),
                    topic,
                    durability_spelling(qos.durability)
                );
            }
            PolicyVerdict::TakeSilently => {}
        }
        granted.durability = declared;
    }
    Ok(granted)
}

/// issue 1608 -- the PUBLISHER counterpart of [`honour`]: the QoS a publisher
/// registers with once the contract has had its say.
///
/// The rule is `policy_verdict`'s, with the POLICY direction reversed from a
/// subscription's because RxO is directional: a publisher that asked for less
/// than the contract declares is RAISED to the declaration (the build reserved
/// it, and every reader that matched the lower offer still matches), and one
/// that asked for more is REFUSED (lowering it could break a match, keeping it
/// rides storage nobody reserved). The depth rule is the subscription's.
///
/// Called at every publisher registration -- the Rust seams AND the C/C++ FFI
/// seams. Unlike a subscription, a publisher has no call-site seam in C or C++
/// that already took the declaration (`NROS_SUBSCRIBE`'s three-argument form
/// has no publisher counterpart, and the compile-time table carries no
/// publisher rows), so the FFI seams honour too rather than strict-checking: a
/// strict check there would refuse every `create_publisher_in<M>(topic)` whose
/// default profile the contract refines.
///
/// `#[inline]` for [`honour`]'s measured reason: a `None` table must fold the
/// call to `Ok(qos)` so an image with no contract is byte-identical.
#[inline]
pub fn honour_publisher(
    type_name: &str,
    topic: &str,
    qos: QoSProfile,
) -> Result<QoSProfile, NodeError> {
    let Some(rows) = DECLARED_PUBLISHER_QOS_ROWS else {
        return Ok(qos);
    };
    honour_role_in(rows, type_name, topic, qos, Role::Publisher)
}

/// issue 1608 -- the declared row for a PUBLISHER on `(type_name, topic)`, or
/// `None`. A table of its own because a publisher and a subscription on one
/// `(type, topic)` are two endpoints with two declarations.
pub fn declared_publisher(type_name: &str, topic: &str) -> Option<DeclaredEndpoint> {
    row_in(DECLARED_PUBLISHER_QOS_ROWS?, type_name, topic).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row with only a depth stated -- the shape every road has carried
    /// since phase-454 W10.
    const fn depth_row(type_name: &'static str, topic: &'static str, d: u32) -> DeclaredEndpoint {
        DeclaredEndpoint {
            type_name,
            topic,
            depth: Some(d),
            reliability: None,
            durability: None,
        }
    }

    /// The shape `build.rs` writes: the ROS spelling and the DDS-mangled one,
    /// for each declared endpoint.
    const ROWS: &[DeclaredEndpoint] = &[
        depth_row("std_msgs/msg/Int32", "/chatter", 1),
        depth_row("std_msgs::msg::dds_::Int32_", "/chatter", 1),
    ];

    /// issue 1256 -- a subscription that declares BOTH policies and no depth,
    /// which is a row the depth-only table could not carry at all.
    const POLICY_ROWS: &[DeclaredEndpoint] = &[DeclaredEndpoint {
        type_name: "std_msgs/msg/Int32",
        topic: "/sensor",
        depth: None,
        reliability: Some(QoSReliabilityPolicy::BestEffort),
        durability: Some(QoSDurabilityPolicy::Volatile),
    }];

    /// ...and one that declares the STRONG value of each.
    const STRONG_ROWS: &[DeclaredEndpoint] = &[DeclaredEndpoint {
        type_name: "std_msgs/msg/Int32",
        topic: "/latched",
        depth: None,
        reliability: Some(QoSReliabilityPolicy::Reliable),
        durability: Some(QoSDurabilityPolicy::TransientLocal),
    }];

    #[test]
    fn either_type_spelling_finds_the_row() {
        assert_eq!(depth_in(ROWS, "std_msgs/msg/Int32", "/chatter"), Some(1));
        assert_eq!(
            depth_in(ROWS, "std_msgs::msg::dds_::Int32_", "/chatter"),
            Some(1),
            "a generated Rust message class reports the DDS-mangled TYPE_NAME; a table that \
             carried only the ROS spelling would match nothing and check nothing"
        );
    }

    #[test]
    fn a_relative_topic_matches_the_declared_absolute_one() {
        assert_eq!(depth_in(ROWS, "std_msgs/msg/Int32", "chatter"), Some(1));
    }

    #[test]
    fn absence_is_not_a_depth() {
        assert_eq!(
            depth_in(ROWS, "std_msgs/msg/Int32", "/undeclared"),
            None,
            "an endpoint nobody declared has no depth -- not 0, and not the ROS default of 10"
        );
        assert_eq!(
            depth_in(ROWS, "std_msgs/msg/Bool", "/chatter"),
            None,
            "the table is keyed on the PAIR: a declared topic under another type is not a hit, \
             or one declaration would size every type carried on that topic"
        );
        assert_eq!(depth_in(&[], "std_msgs/msg/Int32", "/chatter"), None);
        assert_eq!(
            depth_in(POLICY_ROWS, "std_msgs/msg/Int32", "/sensor"),
            None,
            "a row that declares only policies has declared no depth"
        );
    }

    #[test]
    fn a_namespace_is_a_different_endpoint() {
        assert_eq!(
            depth_in(ROWS, "std_msgs/msg/Int32", "/ns/chatter"),
            None,
            "only ONE leading slash is ignored; matching a namespaced topic against a bare \
             declaration would check a row nobody meant"
        );
    }

    /// phase-454 W13 — the TAKE. An unstated call site asks the ROS default 10; a
    /// contract that declares 1 is the size the build reserved, so the
    /// registration comes back at 1 and every other policy is untouched.
    ///
    /// Untouched matters as much as the depth: `honour` rebuilds the profile a
    /// backend is handed, so a reliability or durability quietly reset here
    /// would change the wire behaviour of every declared endpoint.
    #[test]
    fn an_endpoint_asking_deeper_than_its_declaration_registers_at_the_declared_depth() {
        let asked = QoSProfile::QOS_PROFILE_DEFAULT;
        assert_eq!(
            asked.depth, 10,
            "the ROS default an unstated call site gets"
        );
        let granted = honour_in(ROWS, "std_msgs/msg/Int32", "/chatter", asked)
            .expect("taking the declared depth is not an error");
        assert_eq!(
            granted.depth, 1,
            "the contract declares KEEP_LAST(1) and the build reserved for it; a registration \
             that kept its 10 would claim a receive region the arena never bought"
        );
        assert_eq!(granted.reliability, asked.reliability);
        assert_eq!(granted.durability, asked.durability);
        assert_eq!(granted.history, asked.history);
        assert_eq!(granted.deadline_ms, asked.deadline_ms);
        assert_eq!(granted.lifespan_ms, asked.lifespan_ms);
    }

    /// The DDS-mangled spelling is the one a generated Rust message class
    /// carries, so it is the one a typed call site actually hands `honour` —
    /// a take that only matched the ROS spelling would fire for nothing.
    #[test]
    fn the_take_matches_the_spelling_a_typed_call_site_carries() {
        let granted = honour_in(
            ROWS,
            "std_msgs::msg::dds_::Int32_",
            "/chatter",
            QoSProfile::QOS_PROFILE_DEFAULT,
        )
        .expect("admissible");
        assert_eq!(granted.depth, 1);
    }

    /// phase-454 W13 — the REFUSAL, and the reason the rule is asymmetric.
    ///
    /// A registration asking for LESS than the declaration fits inside what the
    /// build reserved, so nothing overruns — but taking would WIDEN a queue the
    /// code narrowed on purpose (`create_subscription_viewable` requires
    /// KEEP_LAST(1)), and the image has paid for slots nothing wants. Two
    /// statements about one fact disagreeing, with no default that explains
    /// either: RFC-0100 D8.
    #[test]
    fn a_declaration_deeper_than_the_registration_is_refused_not_taken() {
        const DEEP: &[DeclaredEndpoint] = &[depth_row("std_msgs/msg/Int32", "/chatter", 20)];
        let mut asked = QoSProfile::QOS_PROFILE_DEFAULT;
        asked.depth = 10;
        assert_eq!(
            honour_in(DEEP, "std_msgs/msg/Int32", "/chatter", asked),
            Err(NodeError::DeclaredDepthMismatch),
            "THE control for the other direction: declared 20, registered 10. If this returns \
             Ok the rule has collapsed into an unconditional take, and a contract can silently \
             deepen any queue in the image."
        );
    }

    /// Equal is not a divergence, and an undeclared endpoint is not one either
    /// — the two arms that must stay silent, or every image in the tree gains
    /// a report it cannot act on.
    #[test]
    fn agreement_and_absence_both_pass_the_profile_through_unchanged() {
        let mut asked = QoSProfile::QOS_PROFILE_DEFAULT;
        asked.depth = 1;
        assert_eq!(
            honour_in(ROWS, "std_msgs/msg/Int32", "/chatter", asked),
            Ok(asked)
        );
        let deep = QoSProfile::QOS_PROFILE_DEFAULT;
        assert_eq!(
            honour_in(ROWS, "std_msgs/msg/Int32", "/undeclared", deep),
            Ok(deep),
            "absence is not zero and it is not ten either -- an endpoint nobody declared keeps \
             exactly the profile the call site built"
        );
        assert_eq!(
            honour_in(&[], "std_msgs/msg/Int32", "/chatter", deep),
            Ok(deep)
        );
    }

    /// issue 1256 — the policy TAKE. The default profile asks RELIABLE and
    /// VOLATILE; a contract declaring `best_effort` is what the build reserved
    /// (no reliable stream history on XRCE), so an unstated call site comes back
    /// best-effort. Durability agrees, so it is untouched, and so is the depth
    /// the row does not state.
    #[test]
    fn a_declared_best_effort_is_taken_by_a_call_site_that_asked_reliable() {
        let asked = QoSProfile::QOS_PROFILE_DEFAULT;
        assert_eq!(asked.reliability, QoSReliabilityPolicy::Reliable);
        let granted = honour_in(POLICY_ROWS, "std_msgs/msg/Int32", "/sensor", asked)
            .expect("a weaker declared reliability is taken, not refused");
        assert_eq!(granted.reliability, QoSReliabilityPolicy::BestEffort);
        assert_eq!(granted.durability, QoSDurabilityPolicy::Volatile);
        assert_eq!(
            granted.depth, asked.depth,
            "the row states no depth, so the depth the call site asked stands"
        );
    }

    /// issue 1256 — the policy REFUSAL, the control for the take above. A call
    /// site that asked `best_effort` against a contract that declares
    /// `reliable` narrowed on purpose; taking would widen it.
    #[test]
    fn a_declared_reliable_is_refused_to_a_call_site_that_asked_best_effort() {
        let mut asked = QoSProfile::QOS_PROFILE_DEFAULT;
        asked.reliability = QoSReliabilityPolicy::BestEffort;
        assert_eq!(
            honour_in(STRONG_ROWS, "std_msgs/msg/Int32", "/latched", asked),
            Err(NodeError::DeclaredQosMismatch),
            "declared reliable, asked best_effort. If this returns Ok the policy rule has \
             collapsed into an unconditional take."
        );
        // And durability on its own: reliability agrees, durability is weaker.
        let mut asked = QoSProfile::QOS_PROFILE_DEFAULT;
        asked.durability = QoSDurabilityPolicy::Volatile;
        assert_eq!(
            honour_in(STRONG_ROWS, "std_msgs/msg/Int32", "/latched", asked),
            Err(NodeError::DeclaredQosMismatch),
            "declared transient_local, asked volatile"
        );
    }

    /// issue 1256 — a call site that stated NO opinion takes the declaration.
    #[test]
    fn a_system_default_policy_takes_the_declaration() {
        let mut asked = QoSProfile::QOS_PROFILE_DEFAULT;
        asked.reliability = QoSReliabilityPolicy::SystemDefault;
        asked.durability = QoSDurabilityPolicy::SystemDefault;
        let granted = honour_in(STRONG_ROWS, "std_msgs/msg/Int32", "/latched", asked)
            .expect("no opinion is not a disagreement");
        assert_eq!(granted.reliability, QoSReliabilityPolicy::Reliable);
        assert_eq!(granted.durability, QoSDurabilityPolicy::TransientLocal);
    }

    /// issue 1256 — the FFI seams' strict check covers the policies too. C and
    /// C++ already took the declaration at their call site, so ANY disagreement
    /// reaching here is real, in either direction.
    #[test]
    fn the_strict_check_refuses_a_policy_disagreement_in_either_direction() {
        let agrees = {
            let mut q = QoSProfile::QOS_PROFILE_DEFAULT;
            q.reliability = QoSReliabilityPolicy::BestEffort;
            q
        };
        assert_eq!(
            check_in(POLICY_ROWS, "std_msgs/msg/Int32", "/sensor", &agrees),
            Ok(())
        );
        assert_eq!(
            check_in(
                POLICY_ROWS,
                "std_msgs/msg/Int32",
                "/sensor",
                &QoSProfile::QOS_PROFILE_DEFAULT
            ),
            Err(NodeError::DeclaredQosMismatch),
            "declared best_effort, registered reliable"
        );
        let mut tl = agrees;
        tl.durability = QoSDurabilityPolicy::TransientLocal;
        assert_eq!(
            check_in(POLICY_ROWS, "std_msgs/msg/Int32", "/sensor", &tl),
            Err(NodeError::DeclaredQosMismatch),
            "declared volatile, registered transient_local"
        );
        assert_eq!(
            check_in(
                ROWS,
                "std_msgs/msg/Int32",
                "/chatter",
                &QoSProfile::QOS_PROFILE_DEFAULT
            ),
            Err(NodeError::DeclaredDepthMismatch),
            "and the depth is still refused as a DEPTH"
        );
    }

    /// issue 1608 -- a PUBLISHER row declaring the STRONG value of each
    /// policy: a latched topic, which is what a zenoh image spends a queryable
    /// slot on.
    const PUB_LATCHED: &[DeclaredEndpoint] = &[DeclaredEndpoint {
        type_name: "std_msgs/msg/Int32",
        topic: "/latched",
        depth: Some(1),
        reliability: Some(QoSReliabilityPolicy::Reliable),
        durability: Some(QoSDurabilityPolicy::TransientLocal),
    }];

    /// ...and one declaring the WEAK value of each: a sensor stream.
    const PUB_SENSOR: &[DeclaredEndpoint] = &[DeclaredEndpoint {
        type_name: "std_msgs/msg/Int32",
        topic: "/sensor",
        depth: None,
        reliability: Some(QoSReliabilityPolicy::BestEffort),
        durability: Some(QoSDurabilityPolicy::Volatile),
    }];

    fn honour_pub(
        rows: &[DeclaredEndpoint],
        topic: &str,
        qos: QoSProfile,
    ) -> Result<QoSProfile, NodeError> {
        honour_role_in(rows, "std_msgs/msg/Int32", topic, qos, Role::Publisher)
    }

    /// issue 1608 -- the publisher TAKE. A default-profile publisher (VOLATILE,
    /// KEEP_LAST(10)) on a topic the contract declares `transient_local` with
    /// depth 1 registers as transient_local, depth 1: the offer is RAISED to
    /// what the build reserved (the queryable slot and one retained sample),
    /// and every reader that matched a volatile writer still matches.
    #[test]
    fn a_publisher_is_raised_to_a_stronger_declared_policy() {
        let asked = QoSProfile::QOS_PROFILE_DEFAULT;
        assert_eq!(asked.durability, QoSDurabilityPolicy::Volatile);
        let granted = honour_pub(PUB_LATCHED, "/latched", asked)
            .expect("a publisher asking LESS than its declaration is raised, not refused");
        assert_eq!(granted.durability, QoSDurabilityPolicy::TransientLocal);
        assert_eq!(granted.reliability, QoSReliabilityPolicy::Reliable);
        assert_eq!(granted.depth, 1, "the depth rule is the subscription's");
        let mut be = asked;
        be.reliability = QoSReliabilityPolicy::BestEffort;
        assert_eq!(
            honour_pub(PUB_LATCHED, "/latched", be).map(|q| q.reliability),
            Ok(QoSReliabilityPolicy::Reliable),
            "a best_effort writer is raised to the declared reliable"
        );
    }

    /// issue 1608 -- the publisher REFUSAL, and THE control for the direction.
    ///
    /// A publisher asking for MORE than the contract declares is refused: taking
    /// the declaration would WEAKEN its offer (a reader requesting what the code
    /// offered stops matching) and keeping the ask rides storage the build did
    /// not reserve. This is the OPPOSITE direction from a subscription -- the
    /// last assertion runs the same row and profile through the subscription
    /// rule and gets a take, so a rule copied across without reversing fails
    /// here rather than in an image.
    #[test]
    fn a_publisher_asking_more_than_its_declaration_is_refused() {
        let asked = QoSProfile::QOS_PROFILE_DEFAULT;
        assert_eq!(asked.reliability, QoSReliabilityPolicy::Reliable);
        assert_eq!(
            honour_pub(PUB_SENSOR, "/sensor", asked),
            Err(NodeError::DeclaredQosMismatch),
            "declared best_effort, asked reliable: lowering the offer could break a match"
        );
        let mut tl = asked;
        tl.reliability = QoSReliabilityPolicy::BestEffort;
        tl.durability = QoSDurabilityPolicy::TransientLocal;
        assert_eq!(
            honour_pub(PUB_SENSOR, "/sensor", tl),
            Err(NodeError::DeclaredQosMismatch),
            "declared volatile, asked transient_local: the build counted no queryable slot"
        );
        let granted = honour_in(PUB_SENSOR, "std_msgs/msg/Int32", "/sensor", asked)
            .expect("the SUBSCRIPTION rule takes the same disagreement");
        assert_eq!(granted.reliability, QoSReliabilityPolicy::BestEffort);
    }

    /// issue 1608 -- agreement, no opinion, and absence stay silent and
    /// unchanged for a publisher too.
    #[test]
    fn a_publisher_that_agrees_or_has_no_opinion_or_no_row_passes() {
        let mut agrees = QoSProfile::QOS_PROFILE_DEFAULT;
        agrees.reliability = QoSReliabilityPolicy::BestEffort;
        assert_eq!(honour_pub(PUB_SENSOR, "/sensor", agrees), Ok(agrees));
        let mut none = QoSProfile::QOS_PROFILE_DEFAULT;
        none.reliability = QoSReliabilityPolicy::SystemDefault;
        none.durability = QoSDurabilityPolicy::SystemDefault;
        let granted = honour_pub(PUB_LATCHED, "/latched", none).expect("no opinion");
        assert_eq!(granted.durability, QoSDurabilityPolicy::TransientLocal);
        let deep = QoSProfile::QOS_PROFILE_DEFAULT;
        assert_eq!(honour_pub(PUB_LATCHED, "/undeclared", deep), Ok(deep));
        assert_eq!(
            honour_pub(PUB_LATCHED, "/latched", {
                let mut q = deep;
                q.durability = QoSDurabilityPolicy::TransientLocal;
                q.depth = 0;
                q
            }),
            Err(NodeError::DeclaredDepthMismatch),
            "a declaration DEEPER than the ask is refused for a publisher as for a subscription"
        );
    }

    /// The public entry point over THIS build's table. In-tree that table is
    /// `None` (no contract reaches a unit-test build), so this asserts the
    /// no-declaration behaviour rather than a lookup -- and says so, because a
    /// reader would otherwise take it for coverage of the lookup above.
    #[cfg(not(nros_declared_qos_table))]
    #[test]
    fn an_image_that_declares_nothing_checks_nothing() {
        assert!(
            DECLARED_QOS_ROWS.is_none(),
            "`nros_declared_qos_table` is unset, so the build wrote no table. If this fires, \
             the cfg and the const have come apart and the declared-image test below is \
             being skipped on a build that HAS a table."
        );
        assert_eq!(declared_depth("std_msgs/msg/Int32", "/chatter"), None);
        assert_eq!(
            check(
                "std_msgs/msg/Int32",
                "/chatter",
                &QoSProfile::QOS_PROFILE_DEFAULT
            ),
            Ok(())
        );
        // phase-454 W13 — and the take is inert too, which is acceptance 4: an
        // image with no contract registers exactly the profile it built.
        let asked = QoSProfile::QOS_PROFILE_DEFAULT;
        assert_eq!(honour("std_msgs/msg/Int32", "/chatter", asked), Ok(asked));
        // issue 1608 -- and so is the publisher take.
        assert!(DECLARED_PUBLISHER_QOS_ROWS.is_none());
        assert_eq!(
            honour_publisher("std_msgs/msg/Int32", "/chatter", asked),
            Ok(asked)
        );
    }

    /// THE NEGATIVE CONTROL, over the build's OWN table.
    ///
    /// Everything above tests `depth_in` against a table this module wrote,
    /// which proves the search and nothing about DELIVERY: a carrier cmake
    /// never forwards, a `build.rs` parse that drops every row, and a table
    /// keyed on a spelling no call site writes all look exactly like "no
    /// mismatch here". Six mechanisms in this campaign have been correct and
    /// unreachable for precisely that reason.
    ///
    /// So this module is compiled only where `build.rs` wrote rows, and
    /// `just check declared-qos-registration` builds this crate with
    /// `NROS_ENTITY_DECLARED_DEPTHS` set to make that happen. It also asserts
    /// that the test does NOT exist without the variable -- otherwise a cfg
    /// that was always on would make the lane pass while proving nothing.
    #[cfg(nros_declared_qos_table)]
    mod declared_image {
        use super::*;

        #[test]
        fn refuses_a_disagreeing_depth_and_accepts_an_agreeing_one() {
            let rows = DECLARED_QOS_ROWS.expect("the cfg says this build wrote a table");
            assert!(
                !rows.is_empty(),
                "an empty table with the cfg set is a parse that dropped every row -- which \
                 would disable the check while reading as if it were on"
            );
            assert_eq!(
                declared_depth("std_msgs/msg/Int32", "/chatter"),
                Some(1),
                "the lane forwards `std_msgs/msg/Int32|/chatter=1`; if this is None the \
                 carrier, the parse or the key spelling is wrong, and every registration in \
                 a real image would go unchecked"
            );
            assert_eq!(
                declared_depth("std_msgs::msg::dds_::Int32_", "/chatter"),
                Some(1),
                "build.rs must emit the DDS-mangled spelling too -- it is what a generated \
                 Rust message class reports as TYPE_NAME, and the only spelling a typed call \
                 site ever hands `check`"
            );
            let mut one = QoSProfile::QOS_PROFILE_DEFAULT;
            one.depth = 1;
            assert_eq!(check("std_msgs/msg/Int32", "/chatter", &one), Ok(()));
            assert_eq!(
                check(
                    "std_msgs::msg::dds_::Int32_",
                    "/chatter",
                    &QoSProfile::QOS_PROFILE_DEFAULT
                ),
                Err(NodeError::DeclaredDepthMismatch),
                "THE control: declared 1, registered 10. If this passes, the registration \
                 check does nothing on a real image and every lane above it stays green."
            );
            assert_eq!(
                check(
                    "std_msgs/msg/Int32",
                    "/undeclared",
                    &QoSProfile::QOS_PROFILE_DEFAULT
                ),
                Ok(()),
                "and an endpoint nobody declared is still not an error"
            );
        }

        /// phase-454 W13 — the TAKE, over the build's own table.
        ///
        /// The sibling above proves the ROWS arrived. This proves the thing a
        /// Rust image actually depends on: that an unstated call site's
        /// `QoSProfile::default()` comes back at the DECLARED depth. A table
        /// that is delivered and never consulted by the registration would pass
        /// every assertion above it, which is the shape this campaign keeps
        /// finding.
        ///
        /// `just check declared-qos-registration` runs this against BOTH
        /// carriers — the cmake road's `NROS_ENTITY_DECLARED_DEPTHS` and the
        /// cargo leaf road's sizing descriptor — because a carrier that reaches
        /// one road and not the other is exactly how W10 shipped with its
        /// registration check unable to compare anything on the road W12 taught
        /// to read a contract.
        #[test]
        fn an_unstated_call_site_registers_at_the_declared_depth() {
            let asked = QoSProfile::QOS_PROFILE_DEFAULT;
            assert_eq!(
                asked.depth, 10,
                "the profile `Node::create_subscription` hands a registration when the call \
                 site states none"
            );
            let granted = honour("std_msgs::msg::dds_::Int32_", "/chatter", asked)
                .expect("the declared depth is shallower than the ask, so it is TAKEN");
            assert_eq!(
                granted.depth, 1,
                "this build's table declares KEEP_LAST(1) for `/chatter`; if this is 10 the \
                 registration ignored the declaration and the image runs a depth the build did \
                 not reserve for"
            );
        }

        /// issue 1256 — the two POLICIES, over the build's own table.
        ///
        /// Compiled only where the build's table states a policy, which is the
        /// DESCRIPTOR carrier (`tests/fixtures/declared-qos-descriptor.toml`
        /// declares `/status` best-effort and volatile): the env carrier
        /// `NROS_ENTITY_DECLARED_DEPTHS` is depth-only by design, and no knob is
        /// added for the policies. `just check declared-qos-registration`
        /// therefore expects THREE tests on the descriptor step and two on the
        /// env step -- so a descriptor whose policies stopped reaching the table
        /// fails that lane instead of passing two of three.
        #[cfg(nros_declared_qos_policy_table)]
        #[test]
        fn a_declared_policy_reaches_the_registration_from_the_descriptor() {
            assert_eq!(
                declared_reliability("std_msgs::msg::dds_::Int32_", "/status"),
                Some(QoSReliabilityPolicy::BestEffort),
                "the descriptor states `reliability = \"best_effort\"` for `/status`; if this \
                 is None the policies are not reaching DECLARED_QOS_ROWS and no Rust image \
                 compares them with its code"
            );
            assert_eq!(
                declared_durability("std_msgs/msg/Int32", "/status"),
                Some(QoSDurabilityPolicy::Volatile)
            );
            let granted = honour(
                "std_msgs::msg::dds_::Int32_",
                "/status",
                QoSProfile::QOS_PROFILE_DEFAULT,
            )
            .expect("a declared best_effort is TAKEN by a call site that asked reliable");
            assert_eq!(granted.reliability, QoSReliabilityPolicy::BestEffort);
            assert_eq!(
                check(
                    "std_msgs/msg/Int32",
                    "/status",
                    &QoSProfile::QOS_PROFILE_DEFAULT
                ),
                Err(NodeError::DeclaredQosMismatch),
                "and the FFI seams' strict check refuses the same disagreement"
            );
        }

        /// issue 1608 -- a declared PUBLISHER, over the build's own table.
        ///
        /// Compiled only where the descriptor delivered a publisher row
        /// (`tests/fixtures/declared-qos-descriptor.toml` declares `/latched`
        /// transient_local, depth 1). `just check declared-qos-registration`
        /// expects FOUR tests on the descriptor step -- so a descriptor whose
        /// publisher rows stopped reaching `DECLARED_PUBLISHER_QOS_ROWS` fails
        /// the lane instead of passing three of four.
        #[cfg(nros_declared_publisher_qos_table)]
        #[test]
        fn a_declared_publisher_reaches_the_registration_from_the_descriptor() {
            let row = declared_publisher("std_msgs::msg::dds_::Int32_", "/latched").expect(
                "the descriptor declares a publisher on `/latched`; if this is None its \
                 publisher rows are not reaching DECLARED_PUBLISHER_QOS_ROWS",
            );
            assert_eq!(row.durability, Some(QoSDurabilityPolicy::TransientLocal));
            assert!(
                declared_publisher("std_msgs/msg/Int32", "/chatter").is_none(),
                "`/chatter` is declared for a SUBSCRIPTION only; a publisher table that \
                 carried it would hold every publisher to a reader's declaration"
            );
            let granted = honour_publisher(
                "std_msgs::msg::dds_::Int32_",
                "/latched",
                QoSProfile::QOS_PROFILE_DEFAULT,
            )
            .expect("a default publisher is raised to the declared transient_local");
            assert_eq!(granted.durability, QoSDurabilityPolicy::TransientLocal);
            assert_eq!(granted.depth, 1);
            let mut be = QoSProfile::QOS_PROFILE_DEFAULT;
            be.durability = QoSDurabilityPolicy::TransientLocal;
            assert_eq!(
                honour_publisher("std_msgs/msg/Int32", "/plain", be),
                Err(NodeError::DeclaredQosMismatch),
                "the descriptor declares `/plain` volatile; a transient_local publisher there \
                 asks for a queryable slot the build never counted"
            );
        }
    }
}
