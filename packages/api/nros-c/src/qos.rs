//! QoS (Quality of Service) settings for the C API.
//!
//! # The two sentinels, and why they are here (issue 1437, phase-444)
//!
//! Until phase-444 the four enums below carried CONCRETE VALUES ONLY. That
//! was sound while a `nros_qos_t` could only ever travel one way — a C caller
//! WRITES a profile and hands it to `nros_*_init_with_qos` — because a
//! request is always a concrete demand.
//!
//! `nros_*_get_actual_qos` reverses the direction: the struct now also
//! carries what a BACKEND GRANTED, and a grant has two answers a request
//! never has. `rmw_vtable.h` has always owed both:
//!
//! * `*_UNKNOWN` — **the backend looked and cannot say.** Upstream's
//!   `RMW_QOS_POLICY_*_UNKNOWN`, and what `rmw_publisher_get_actual_qos`
//!   requires for a policy an implementation cannot determine. An ABSENCE,
//!   not a value. A profile that is unknown in EVERY field is the honest
//!   answer from a backend with no read-back at all (XRCE has none).
//! * `*_SYSTEM_DEFAULT` — **nobody stated this policy, so the middleware
//!   chose.** Upstream's `RMW_QOS_POLICY_*_SYSTEM_DEFAULT`; discriminant 0 in
//!   the RMW ABI, which is why a backend can report it without meaning to.
//!
//! The two are DIFFERENT statements and the C API had one word for neither,
//! so a read-back had to either invent a concrete value or conflate them.
//! Both are now spellable. They are appended, never renumbered: these
//! discriminants are baked into shipped images.
//!
//! # These discriminants are NOT the RMW ABI's — convert, never cast
//!
//! This is the application API's vocabulary. `<nros/rmw_entity.h>` has its
//! own, and **all four enums are skewed against it**, not just the one that
//! is famous for it:
//!
//! | policy | this header | `NROS_RMW_*` (rmw_entity.h) |
//! |---|---|---|
//! | reliability | BEST_EFFORT 0, RELIABLE 1 | SYSTEM_DEFAULT 0, RELIABLE 1, BEST_EFFORT 2 |
//! | durability | VOLATILE 0, TRANSIENT_LOCAL 1 | SYSTEM_DEFAULT 0, TRANSIENT_LOCAL 1, VOLATILE 2 |
//! | history | KEEP_LAST 0, KEEP_ALL 1 | SYSTEM_DEFAULT 0, KEEP_LAST 1, KEEP_ALL 2 |
//! | liveliness | NONE 0, AUTOMATIC 1, MANUAL_BY_TOPIC 2, MANUAL_BY_NODE 3 | SYSTEM_DEFAULT 0, AUTOMATIC 1, MANUAL_BY_NODE 2, MANUAL_BY_TOPIC 3 |
//!
//! Reliability and durability AGREE on exactly one enumerator each and
//! disagree on the other; history agrees on none; liveliness has the two
//! MANUAL kinds transposed (phase-376 W5/B2 fixed the RMW side and left this
//! one, correctly — the value crossing the ABI is the RMW one). So a numeric
//! cast between the two vocabularies is wrong in every enum, and it is wrong
//! SILENTLY: `MANUAL_BY_TOPIC` becomes `MANUAL_BY_NODE` on the wire, and an
//! appended sentinel would become a concrete policy. Every conversion in this
//! file is an explicit match for that reason, and the bridge to the RMW ABI
//! is `nros-rmw`'s `QoSProfile`, never an `as`.

use core::ffi::c_int;

/// QoS reliability policy
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_qos_reliability_t {
    /// Best effort delivery - no guarantees
    NROS_QOS_RELIABILITY_BEST_EFFORT = 0,
    /// Reliable delivery - retransmit if needed
    NROS_QOS_RELIABILITY_RELIABLE = 1,
    /// The caller stated no reliability policy — the middleware chose.
    /// Upstream's `RMW_QOS_POLICY_RELIABILITY_SYSTEM_DEFAULT`.
    NROS_QOS_RELIABILITY_SYSTEM_DEFAULT = 2,
    /// The backend could not determine this policy — an ABSENCE, never a
    /// request. Upstream's `RMW_QOS_POLICY_RELIABILITY_UNKNOWN`.
    NROS_QOS_RELIABILITY_UNKNOWN = 3,
}

/// QoS durability policy
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_qos_durability_t {
    /// Volatile - no persistence
    NROS_QOS_DURABILITY_VOLATILE = 0,
    /// Transient local - persist for late joiners
    NROS_QOS_DURABILITY_TRANSIENT_LOCAL = 1,
    /// The caller stated no durability policy — the middleware chose.
    /// Upstream's `RMW_QOS_POLICY_DURABILITY_SYSTEM_DEFAULT`.
    NROS_QOS_DURABILITY_SYSTEM_DEFAULT = 2,
    /// The backend could not determine this policy — an ABSENCE, never a
    /// request. Upstream's `RMW_QOS_POLICY_DURABILITY_UNKNOWN`.
    NROS_QOS_DURABILITY_UNKNOWN = 3,
}

/// QoS history policy
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_qos_history_t {
    /// Keep last N samples
    NROS_QOS_HISTORY_KEEP_LAST = 0,
    /// Keep all samples
    NROS_QOS_HISTORY_KEEP_ALL = 1,
    /// The caller stated no history policy — the middleware chose.
    /// Upstream's `RMW_QOS_POLICY_HISTORY_SYSTEM_DEFAULT`.
    NROS_QOS_HISTORY_SYSTEM_DEFAULT = 2,
    /// The backend could not determine this policy — an ABSENCE, never a
    /// request. Upstream's `RMW_QOS_POLICY_HISTORY_UNKNOWN`.
    NROS_QOS_HISTORY_UNKNOWN = 3,
}

/// QoS liveliness policy. Phase 109 — matches DDS `LIVELINESS`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_qos_liveliness_t {
    /// No liveliness assertion or tracking.
    ///
    /// This is ALSO the "nobody stated a liveliness policy" slot — it is
    /// discriminant 0, the value the RMW ABI spells
    /// `NROS_RMW_LIVELINESS_SYSTEM_DEFAULT`, and a backend that sees it omits
    /// the DDS call entirely. So this enum gains no separate
    /// `SYSTEM_DEFAULT`: it would be a second name for a value that already
    /// has one, which is how a vocabulary starts disagreeing with itself.
    NROS_QOS_LIVELINESS_NONE = 0,
    /// Backend's keepalive task asserts liveliness automatically.
    NROS_QOS_LIVELINESS_AUTOMATIC = 1,
    /// Application calls `assert_liveliness()` per topic explicitly.
    NROS_QOS_LIVELINESS_MANUAL_BY_TOPIC = 2,
    /// Application calls `assert_liveliness()` at the node level.
    NROS_QOS_LIVELINESS_MANUAL_BY_NODE = 3,
    /// The backend could not determine this policy — an ABSENCE, never a
    /// request. Upstream's `RMW_QOS_POLICY_LIVELINESS_UNKNOWN`.
    NROS_QOS_LIVELINESS_UNKNOWN = 4,
}

/// Full DDS-shaped QoS profile (Phase 109).
///
/// Matches the field set of upstream `rmw_qos_profile_t`. Backends
/// advertise per-policy support; entities created with a profile the
/// active backend can't honour return `NROS_RMW_RET_INCOMPATIBLE_QOS`
/// synchronously at create time — no silent downgrade.
///
/// Zero-valued time-window fields ("off") preserve a cheap default
/// for apps that don't request the policy.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_qos_t {
    /// Reliability policy
    pub reliability: nros_qos_reliability_t,
    /// Durability policy
    pub durability: nros_qos_durability_t,
    /// History policy
    pub history: nros_qos_history_t,
    /// Liveliness policy
    pub liveliness_kind: nros_qos_liveliness_t,
    /// History depth (for KEEP_LAST)
    pub depth: c_int,
    /// Subscriber max-inter-arrival / publisher offered-rate, ms.
    /// `0` = infinite (no deadline check).
    pub deadline_ms: u32,
    /// Sample expiry, ms. `0` = infinite.
    pub lifespan_ms: u32,
    /// Liveliness lease, ms. `0` = infinite.
    pub liveliness_lease_ms: u32,
    /// If non-zero, topic-name encoding skips the `/rt/` ROS prefix.
    pub avoid_ros_namespace_conventions: u8,
    /// Phase 282 (#145) — publisher-side "express" hint: if non-zero, this
    /// publisher's samples bypass transport tx batching (sent immediately
    /// even when the batching knob is on). A transport hint, not a DDS
    /// policy; ignored on subscriptions and by backends without batching.
    pub tx_express: u8,
}

// phase-454 W1 — the `NROS_QOS_*` statics below are bound to
// `QoSProfile::QOS_PROFILE_*` two ways. Their DEPTHS are read from it directly
// (`c_depth`, immediately below), so the compiler is the binding; their
// POLICIES are still literals, and `scripts/check-qos-profile-ssot.py` compares
// them field-by-field. Both halves are needed: a Rust file can consume a const
// and a C header cannot, so the gate has to exist anyway, and where a value CAN
// be consumed it should be, because a derived value cannot drift between gate
// runs.
//
// The liveliness deviation is GONE (issue 1329, 2026-09-12). Every profile
// here carried AUTOMATIC where the SSoT carries the sentinel, declared with a
// mirror-deviation record for `liveliness_kind` and left standing because the
// three application surfaces "move together or not at all". They moved
// together, in one commit: this file,
// `nros-c/include/nros/component.h`'s `nros_c_qos_default()` and
// `nros-cpp`'s `QoS` constructor.
//
// What forced it: issue 1329 gives each C backend its OWN QoS mask instead of
// the union, and XRCE honours no liveliness at all. Stating AUTOMATIC made
// `required_policies()` DEMAND `LIVELINESS_AUTOMATIC` for every C default
// profile — an over-demand against a policy upstream leaves unset — so every C
// application on XRCE would have been refused at entity create by a bit it
// never asked for. The fix is not to let XRCE claim a policy it does not
// implement; it is to stop asking for one.
//
// Wire effect, re-reading W10's own measurements: none. cyclonedds calls
// `dds_qset_liveliness` only for a non-sentinel kind and its own default IS
// `DDS_LIVELINESS_AUTOMATIC`; zenoh's `QosKeyExpr::to_qos_string` leaves the
// liveliness positions empty for every profile; XRCE lowers no liveliness
// field. A caller that WANTS automatic liveliness still states it.

/// The SSoT's depth, narrowed to the width this C API's `depth` field carries.
///
/// issue 1256 / phase-454 W1. The depths below are read out of
/// `QoSProfile::QOS_PROFILE_*`, whose field is a `u32`, and `as c_int` would
/// reinterpret rather than refuse. Const-evaluated, so a profile stating a
/// depth past `c_int` is a COMPILE error at that profile instead of a negative
/// queue depth handed to a backend.
const fn c_depth(depth: u32) -> c_int {
    assert!(
        depth <= c_int::MAX as u32,
        "a QoS profile states a history depth the C API's `int` cannot carry",
    );
    depth as c_int
}

impl Default for nros_qos_t {
    /// The same ten fields `NROS_QOS_DEFAULT` states, by naming it.
    ///
    /// phase-454 W1 — this WAS a second ten-field initialiser, field-identical
    /// to the static below it and held that way by nothing. A hand-mirror at a
    /// distance of thirty lines is still a hand-mirror (issue 0160): the
    /// `tx_express` append reached both only because one author touched both in
    /// one sitting. `nros_qos_t` is `Copy`, so naming the static costs a
    /// register move and removes the copy outright, which is better than
    /// gating it.
    fn default() -> Self {
        NROS_QOS_DEFAULT
    }
}

/// Default QoS profile (matches `rmw_qos_profile_default`).
#[unsafe(no_mangle)]
pub static NROS_QOS_DEFAULT: nros_qos_t = nros_qos_t {
    reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE,
    durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
    history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
    liveliness_kind: nros_qos_liveliness_t::NROS_QOS_LIVELINESS_NONE,
    depth: c_depth(nros_node::QoSProfile::QOS_PROFILE_DEFAULT.depth),
    deadline_ms: 0,
    lifespan_ms: 0,
    liveliness_lease_ms: 0,
    avoid_ros_namespace_conventions: 0,
    tx_express: 0,
};

/// Sensor data QoS profile (best effort, small depth).
#[unsafe(no_mangle)]
pub static NROS_QOS_SENSOR_DATA: nros_qos_t = nros_qos_t {
    reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_BEST_EFFORT,
    durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
    history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
    liveliness_kind: nros_qos_liveliness_t::NROS_QOS_LIVELINESS_NONE,
    depth: c_depth(nros_node::QoSProfile::QOS_PROFILE_SENSOR_DATA.depth),
    deadline_ms: 0,
    lifespan_ms: 0,
    liveliness_lease_ms: 0,
    avoid_ros_namespace_conventions: 0,
    tx_express: 0,
};

/// Services QoS profile (reliable).
#[unsafe(no_mangle)]
pub static NROS_QOS_SERVICES: nros_qos_t = nros_qos_t {
    reliability: nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE,
    durability: nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE,
    history: nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
    liveliness_kind: nros_qos_liveliness_t::NROS_QOS_LIVELINESS_NONE,
    depth: c_depth(nros_node::QoSProfile::QOS_PROFILE_SERVICES_DEFAULT.depth),
    deadline_ms: 0,
    lifespan_ms: 0,
    liveliness_lease_ms: 0,
    avoid_ros_namespace_conventions: 0,
    tx_express: 0,
};

/// The name of ONE QoS policy — phase-417 G7, the C half of
/// `rclcpp::qos_policy_kind_to_cstr`.
///
/// @param policy One `NROS_RMW_QOS_POLICY_*` bit from
///        `<nros/rmw_entity.h>` — the same twelve bits a backend advertises
///        through the vtable's `supported_qos_policies` slot, and the same
///        twelve `QoSPolicyMask` carries in Rust. `check-qos-mask-derivation`
///        holds those two vocabularies equal name for name and bit for bit, so
///        this function has exactly one table behind it however you reach it.
///
/// @return A borrowed, NUL-terminated, `'static` string — `"RELIABILITY"`,
///         `"DEADLINE"`, … — or `NULL` when @p policy is not exactly one
///         policy bit. Never freed, never allocated: the bytes are in flash,
///         which is what makes this usable from a freestanding image.
///
/// NULL rather than `"UNKNOWN"` for zero and for a union: both are real
/// answers, and a caller that prints a name it was handed must be able to tell
/// "no policy was identified" from a policy that was. `rclcpp`'s own version
/// returns `"Invalid"` for its invalid enumerator; we have no invalid
/// enumerator to return a word for, because our vocabulary is a bitmask and
/// the empty mask is a legitimate value.
///
/// This is why the name exists at all (ledger `cpp:qos_policy_kind_to_cstr`):
/// an `INCOMPATIBLE_QOS` return names no policy by itself, and naming the
/// policy IS the diagnostic.
/// No table of its own — the pointer is `QoSPolicyMask::NAMED`'s own bytes,
/// which is the whole point of the row. `nros-c` restating the twelve
/// identifiers here would be the parallel table this campaign has paid for
/// twice (the RMW parity map's 28 stale slots, the layout gate's three
/// authored type names).
#[unsafe(no_mangle)]
pub extern "C" fn nros_qos_policy_kind_to_cstr(policy: u32) -> *const core::ffi::c_char {
    match nros_rmw::QoSPolicyMask(policy).policy_name_cstr() {
        Some(name) => name.as_ptr(),
        None => core::ptr::null(),
    }
}

impl nros_qos_t {
    /// Convert to nros QoSProfile
    pub(crate) fn to_qos_settings(self) -> nros_node::QoSProfile {
        use nros_node::{
            QoSDurabilityPolicy, QoSHistoryPolicy, QoSLivelinessPolicy, QoSReliabilityPolicy,
        };

        let reliability = match self.reliability {
            nros_qos_reliability_t::NROS_QOS_RELIABILITY_BEST_EFFORT => {
                QoSReliabilityPolicy::BestEffort
            }
            nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE => QoSReliabilityPolicy::Reliable,
            nros_qos_reliability_t::NROS_QOS_RELIABILITY_SYSTEM_DEFAULT => {
                QoSReliabilityPolicy::SystemDefault
            }
            nros_qos_reliability_t::NROS_QOS_RELIABILITY_UNKNOWN => QoSReliabilityPolicy::Unknown,
        };

        let durability = match self.durability {
            nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE => QoSDurabilityPolicy::Volatile,
            nros_qos_durability_t::NROS_QOS_DURABILITY_TRANSIENT_LOCAL => {
                QoSDurabilityPolicy::TransientLocal
            }
            nros_qos_durability_t::NROS_QOS_DURABILITY_SYSTEM_DEFAULT => {
                QoSDurabilityPolicy::SystemDefault
            }
            nros_qos_durability_t::NROS_QOS_DURABILITY_UNKNOWN => QoSDurabilityPolicy::Unknown,
        };

        let history = match self.history {
            nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST => QoSHistoryPolicy::KeepLast,
            nros_qos_history_t::NROS_QOS_HISTORY_KEEP_ALL => QoSHistoryPolicy::KeepAll,
            nros_qos_history_t::NROS_QOS_HISTORY_SYSTEM_DEFAULT => QoSHistoryPolicy::SystemDefault,
            nros_qos_history_t::NROS_QOS_HISTORY_UNKNOWN => QoSHistoryPolicy::Unknown,
        };

        let liveliness_kind = match self.liveliness_kind {
            nros_qos_liveliness_t::NROS_QOS_LIVELINESS_NONE => QoSLivelinessPolicy::None,
            nros_qos_liveliness_t::NROS_QOS_LIVELINESS_AUTOMATIC => QoSLivelinessPolicy::Automatic,
            nros_qos_liveliness_t::NROS_QOS_LIVELINESS_MANUAL_BY_TOPIC => {
                QoSLivelinessPolicy::ManualByTopic
            }
            nros_qos_liveliness_t::NROS_QOS_LIVELINESS_MANUAL_BY_NODE => {
                QoSLivelinessPolicy::ManualByNode
            }
            nros_qos_liveliness_t::NROS_QOS_LIVELINESS_UNKNOWN => QoSLivelinessPolicy::Unknown,
        };

        nros_node::QoSProfile {
            reliability,
            durability,
            history,
            liveliness_kind,
            depth: self.depth as u32,
            deadline_ms: self.deadline_ms,
            lifespan_ms: self.lifespan_ms,
            liveliness_lease_ms: self.liveliness_lease_ms,
            avoid_ros_namespace_conventions: self.avoid_ros_namespace_conventions != 0,
            tx_express: self.tx_express != 0,
        }
    }

    /// The C-API spelling of a profile that came OUT of the transport —
    /// issue 1437, the inverse of [`to_qos_settings`](Self::to_qos_settings).
    ///
    /// Written for `nros_*_get_actual_qos`, which is the only direction that
    /// produces one: the caller's own request goes the other way. Total over
    /// `QoSProfile`, including the two answers only a grant can carry —
    /// `SystemDefault` and `Unknown` — because a conversion that folded
    /// either onto a concrete policy would report a value where the backend
    /// reported an absence, which is the defect issue 1437 exists to close.
    ///
    /// One arm per variant, no numeric cast: the two vocabularies are skewed
    /// in all four enums (see this module's header).
    pub(crate) fn from_qos_settings(qos: nros_node::QoSProfile) -> Self {
        use nros_node::{
            QoSDurabilityPolicy, QoSHistoryPolicy, QoSLivelinessPolicy, QoSReliabilityPolicy,
        };

        Self {
            reliability: match qos.reliability {
                QoSReliabilityPolicy::BestEffort => {
                    nros_qos_reliability_t::NROS_QOS_RELIABILITY_BEST_EFFORT
                }
                QoSReliabilityPolicy::Reliable => {
                    nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE
                }
                QoSReliabilityPolicy::SystemDefault => {
                    nros_qos_reliability_t::NROS_QOS_RELIABILITY_SYSTEM_DEFAULT
                }
                QoSReliabilityPolicy::Unknown => {
                    nros_qos_reliability_t::NROS_QOS_RELIABILITY_UNKNOWN
                }
            },
            durability: match qos.durability {
                QoSDurabilityPolicy::Volatile => {
                    nros_qos_durability_t::NROS_QOS_DURABILITY_VOLATILE
                }
                QoSDurabilityPolicy::TransientLocal => {
                    nros_qos_durability_t::NROS_QOS_DURABILITY_TRANSIENT_LOCAL
                }
                QoSDurabilityPolicy::SystemDefault => {
                    nros_qos_durability_t::NROS_QOS_DURABILITY_SYSTEM_DEFAULT
                }
                QoSDurabilityPolicy::Unknown => nros_qos_durability_t::NROS_QOS_DURABILITY_UNKNOWN,
            },
            history: match qos.history {
                QoSHistoryPolicy::KeepLast => nros_qos_history_t::NROS_QOS_HISTORY_KEEP_LAST,
                QoSHistoryPolicy::KeepAll => nros_qos_history_t::NROS_QOS_HISTORY_KEEP_ALL,
                QoSHistoryPolicy::SystemDefault => {
                    nros_qos_history_t::NROS_QOS_HISTORY_SYSTEM_DEFAULT
                }
                QoSHistoryPolicy::Unknown => nros_qos_history_t::NROS_QOS_HISTORY_UNKNOWN,
            },
            liveliness_kind: match qos.liveliness_kind {
                QoSLivelinessPolicy::None => nros_qos_liveliness_t::NROS_QOS_LIVELINESS_NONE,
                QoSLivelinessPolicy::Automatic => {
                    nros_qos_liveliness_t::NROS_QOS_LIVELINESS_AUTOMATIC
                }
                QoSLivelinessPolicy::ManualByTopic => {
                    nros_qos_liveliness_t::NROS_QOS_LIVELINESS_MANUAL_BY_TOPIC
                }
                QoSLivelinessPolicy::ManualByNode => {
                    nros_qos_liveliness_t::NROS_QOS_LIVELINESS_MANUAL_BY_NODE
                }
                QoSLivelinessPolicy::Unknown => nros_qos_liveliness_t::NROS_QOS_LIVELINESS_UNKNOWN,
            },
            depth: c_depth_saturating(qos.depth),
            deadline_ms: qos.deadline_ms,
            lifespan_ms: qos.lifespan_ms,
            liveliness_lease_ms: qos.liveliness_lease_ms,
            avoid_ros_namespace_conventions: u8::from(qos.avoid_ros_namespace_conventions),
            tx_express: u8::from(qos.tx_express),
        }
    }
}

/// A backend-reported depth, narrowed to the width this C API carries.
///
/// The compile-time [`c_depth`] cannot serve here — the value is a RUNTIME
/// read-back, so there is no constant to assert on. Saturating rather than
/// wrapping, because `as c_int` on a depth past `INT_MAX` reinterprets into a
/// NEGATIVE queue depth, and a caller comparing it against its request would
/// read that as "the backend shrank my queue to nonsense" instead of "the
/// number does not fit". No in-tree backend can reach this; it is here so
/// that a future one which does cannot get it silently wrong.
const fn c_depth_saturating(depth: u32) -> c_int {
    if depth > c_int::MAX as u32 {
        c_int::MAX
    } else {
        depth as c_int
    }
}

/// Phase 211.H (issue #52) — one per-topic QoS override, the C-ABI mirror of
/// Rust's `nros_rmw::QoSOverride`. The deploy plan lowers a
/// `qos_overrides.<topic>.<role>.<policy>` launch param into a `&'static`
/// array of these, which the entry installs on the node via
/// [`nros_node_set_qos_overrides`](crate::node::nros_node_set_qos_overrides);
/// the node folds the matching entries into each entity's QoS at
/// `create_publisher` / `create_subscription` time (setup-time, before the
/// backend-compat check — no silent downgrade).
///
/// Plain scalar fields only (no `#[repr(C)]` enums) so the C++/cbindgen header
/// is trivially stable and there is no short-enum ABI mirror to keep in sync.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_qos_override_t {
    /// Resolved (remapped) topic the override targets, NUL-terminated UTF-8
    /// (e.g. `"/chatter"`). Matched exactly against the entity's topic.
    pub topic: *const core::ffi::c_char,
    /// `0` = publisher, `1` = subscription. Other values never match.
    pub role: u8,
    /// `0` = reliability, `1` = durability, `2` = history, `3` = depth,
    /// `4` = deadline, `5` = lifespan, `6` = liveliness,
    /// `7` = liveliness_lease_duration. Append-only — these numbers are baked
    /// into shipped images; `nros_rmw::qos_override_policy` is the SSoT.
    pub policy: u8,
    /// Policy-specific value: reliability `0`=best_effort/`1`=reliable;
    /// durability `0`=volatile/`1`=transient_local; history
    /// `0`=keep_last/`1`=keep_all; depth = the KeepLast depth; deadline /
    /// lifespan / liveliness_lease_duration = milliseconds; liveliness =
    /// the `QoSLivelinessPolicy` discriminant
    /// (`0`=none/`1`=automatic/`2`=manual_by_topic/`3`=manual_by_node).
    pub value: u32,
}

/// Role tag for [`apply_qos_overrides`]: `0` = publisher, `1` = subscription
/// (matches [`nros_qos_override_t::role`]).
pub(crate) const QOS_OVERRIDE_ROLE_PUBLISHER: u8 = 0;
pub(crate) const QOS_OVERRIDE_ROLE_SUBSCRIPTION: u8 = 1;

/// Fold any overrides matching `(topic, role)` into `qos`, returning the
/// overridden profile. Mirrors `nros_rmw::QoSProfile::apply_overrides`: a
/// single linear scan, last-write-wins on a duplicate `(topic, role, policy)`,
/// no alloc. `overrides` may be null (`len == 0` ⇒ no-op).
///
/// # Safety
/// `overrides` must be null or point to `len` valid `nros_qos_override_t`, each
/// with a `topic` that is null or a valid NUL-terminated UTF-8 C string for the
/// duration of the call.
pub(crate) unsafe fn apply_qos_overrides(
    mut qos: nros_node::QoSProfile,
    overrides: *const nros_qos_override_t,
    len: usize,
    topic: &str,
    role: u8,
) -> nros_node::QoSProfile {
    if overrides.is_null() || len == 0 {
        return qos;
    }
    let table = unsafe { core::slice::from_raw_parts(overrides, len) };
    for ovr in table {
        if ovr.role != role || ovr.topic.is_null() {
            continue;
        }
        let ovr_topic = match unsafe { core::ffi::CStr::from_ptr(ovr.topic) }.to_str() {
            Ok(s) => s,
            Err(_) => continue,
        };
        if ovr_topic != topic {
            continue;
        }
        // Issue 0303 — ONE decoder lives in `nros-rmw`. This match used to be
        // spelled here with a silent catch-all, so a policy added to the enum
        // reached three languages and not this one.
        if let Some(value) = nros_rmw::decode_qos_override_value(ovr.policy, ovr.value) {
            qos.apply_override_value(value);
        }
    }
    qos
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_node::{QoSDurabilityPolicy, QoSReliabilityPolicy};

    /// phase-417 W5.d — the premises the rclc `_best_effort` presets in
    /// `<nros/{publisher,subscription,service,client}.h>` are built on.
    ///
    /// Those forwarders are `static inline`, so nothing in the Rust test suite
    /// can call them; what IS testable is the relationship between the three
    /// exported profiles they choose between, and that relationship is the part
    /// a reader gets wrong. rclc does NOT build one best-effort profile and use
    /// it everywhere (`rclc/src/rclc/{publisher,subscription,service,client}.c`
    /// @ `10eadcc`):
    ///
    /// * pub/sub `_best_effort` passes `rmw_qos_profile_sensor_data`, whose
    ///   DEPTH is 5 — not "the default with reliability flipped";
    /// * service/client `_best_effort` copies
    ///   `rmw_qos_profile_services_default` and flips one field, so its depth
    ///   stays 10.
    ///
    /// A later "simplification" of the service preset onto
    /// `NROS_QOS_SENSOR_DATA` would compile, ship, and quietly cut a service's
    /// queue depth in half. This is what stands between that and the tree.
    #[test]
    fn the_rclc_best_effort_presets_choose_between_two_different_profiles() {
        assert_eq!(
            NROS_QOS_SENSOR_DATA.reliability,
            nros_qos_reliability_t::NROS_QOS_RELIABILITY_BEST_EFFORT,
        );
        assert_ne!(
            NROS_QOS_SENSOR_DATA.depth, NROS_QOS_DEFAULT.depth,
            "the pub/sub preset is the SENSOR profile, not the default with \
             reliability flipped -- the depth is the difference",
        );
        assert_eq!(NROS_QOS_SENSOR_DATA.depth, 5);

        // The service preset's base. `nros_qos_services_best_effort()` copies
        // this and flips reliability, and the reliable sibling
        // `rclc_service_init_default` reaches NROS_QOS_DEFAULT by passing a
        // NULL qos -- so the pair differs in exactly one field only while these
        // two agree on the rest, which is the claim the headers make.
        assert_eq!(
            NROS_QOS_SERVICES.reliability,
            nros_qos_reliability_t::NROS_QOS_RELIABILITY_RELIABLE,
        );
        assert_eq!(NROS_QOS_SERVICES.depth, NROS_QOS_DEFAULT.depth);
        assert_eq!(NROS_QOS_SERVICES.durability, NROS_QOS_DEFAULT.durability);
        assert_eq!(NROS_QOS_SERVICES.history, NROS_QOS_DEFAULT.history);
    }

    #[test]
    fn apply_qos_overrides_matches_topic_and_role() {
        // best_effort reliability on /chatter for the publisher role.
        let ovr = [nros_qos_override_t {
            topic: c"/chatter".as_ptr(),
            role: QOS_OVERRIDE_ROLE_PUBLISHER,
            policy: 0, // reliability
            value: 0,  // best_effort
        }];
        let base = nros_node::QoSProfile::default(); // Reliable

        // Matching (topic, role) → overridden.
        let got = unsafe {
            apply_qos_overrides(
                base,
                ovr.as_ptr(),
                ovr.len(),
                "/chatter",
                QOS_OVERRIDE_ROLE_PUBLISHER,
            )
        };
        assert_eq!(got.reliability, QoSReliabilityPolicy::BestEffort);

        // Wrong role → untouched.
        let got = unsafe {
            apply_qos_overrides(
                base,
                ovr.as_ptr(),
                ovr.len(),
                "/chatter",
                QOS_OVERRIDE_ROLE_SUBSCRIPTION,
            )
        };
        assert_eq!(got.reliability, QoSReliabilityPolicy::Reliable);

        // Wrong topic → untouched.
        let got = unsafe {
            apply_qos_overrides(
                base,
                ovr.as_ptr(),
                ovr.len(),
                "/other",
                QOS_OVERRIDE_ROLE_PUBLISHER,
            )
        };
        assert_eq!(got.reliability, QoSReliabilityPolicy::Reliable);

        // Null / empty table → no-op.
        let got = unsafe {
            apply_qos_overrides(
                base,
                core::ptr::null(),
                0,
                "/chatter",
                QOS_OVERRIDE_ROLE_PUBLISHER,
            )
        };
        assert_eq!(got.reliability, QoSReliabilityPolicy::Reliable);
    }

    #[test]
    fn apply_qos_overrides_durability_and_depth() {
        let ovr = [
            nros_qos_override_t {
                topic: c"/t".as_ptr(),
                role: QOS_OVERRIDE_ROLE_SUBSCRIPTION,
                policy: 1, // durability
                value: 1,  // transient_local
            },
            nros_qos_override_t {
                topic: c"/t".as_ptr(),
                role: QOS_OVERRIDE_ROLE_SUBSCRIPTION,
                policy: 3, // depth
                value: 42,
            },
        ];
        let got = unsafe {
            apply_qos_overrides(
                nros_node::QoSProfile::default(),
                ovr.as_ptr(),
                ovr.len(),
                "/t",
                QOS_OVERRIDE_ROLE_SUBSCRIPTION,
            )
        };
        assert_eq!(got.durability, QoSDurabilityPolicy::TransientLocal);
        assert_eq!(got.depth, 42);
    }
}
