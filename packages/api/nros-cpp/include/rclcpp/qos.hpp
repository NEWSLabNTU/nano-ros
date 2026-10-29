// nros-cpp: QoS (Quality of Service) profiles
// Freestanding C++ — no STL required

/**
 * @file qos.hpp
 * @ingroup grp_qos
 * @brief `rclcpp::QoS` — full DDS-shaped QoS settings (Phase 108.B.7).
 */

#ifndef NROS_RCLCPP_QOS_HPP
#define NROS_RCLCPP_QOS_HPP

#include <stdint.h>

// Phase 379 W5 — the deadline / lifespan / lease accessors take and return
// `rclcpp::Duration`, so this header names it. `nros.hpp` includes `qos.hpp`
// before `duration.hpp`, so the dependency is spelled HERE rather than left to
// the umbrella's ordering: a header that names a type must be able to be
// included first.
#include "rclcpp/duration.hpp"
// phase-417 G7 — `NROS_PUBLIC`, for the one C entry point this header
// declares (`qos_policy_kind_to_cstr`, below). Macros only; the file includes
// nothing and compiles freestanding, so it costs this header's
// self-containment nothing.
#include "rclc/visibility_control.h"
// phase-417 stage 6 step A — `rclcpp::detail::refuse` and the
// `NROS_RCLCPP_REFUSE_*` diagnostics, used by `SystemDefaultsQoS` below.
// `log.hpp` includes nothing of ours, so this adds no cycle.
#include "rclcpp/logging.hpp"
// `<type_traits>` is GATED, and this is issue 0112's class one step further
// than step A caught it. Zephyr's minimal libcpp ships exactly THREE headers --
// `cstddef`, `cstdint`, `new` (`zephyr/lib/cpp/minimal/include/`) -- so an
// UNGATED include here made `#include <rclcpp/rclcpp.hpp>` fail to compile on every
// Zephyr C++ image. Step A verified against a SYNTHESISED minimal libcpp that
// happened to provide `<type_traits>`, so the lane stayed green.
//
// `is_qos_arg` exists only to disambiguate `rclcpp::Node`'s create_service
// overloads, and that node type is itself hosted-only, so nothing freestanding
// can reach the predicate.
// This header includes NOTHING, deliberately -- see `is_qos_arg` below.

// FFI struct definition — mirrors `nros_cpp_qos_t` in
// nros_cpp_ffi.h. Phase 118.D: guarded by `NROS_CPP_FFI_H`. If
// `nros_cpp_ffi.h` was included earlier (it sets that guard), the
// canonical types are already in scope; otherwise emit local
// definitions so this header stays self-contained for callers that
// don't pull the cbindgen header directly.
//
// The FIELD names below are ABI. `deadline_ms` / `lifespan_ms` /
// `liveliness_lease_ms` are `uint32_t` milliseconds and are mirrored again in
// `rclc/component.h` (issue 0160, gated by `check-ffi-struct-mirrors`). The
// `_ms` suffix on a `QoS` METHOD was renamed in phase-379 W5; the suffix on a
// STRUCT FIELD was not, and must not be — the C ABI carries milliseconds.
#ifndef NROS_CPP_FFI_H
extern "C" {
enum nros_cpp_qos_reliability_t {
    NROS_CPP_QOS_RELIABLE = 0,
    NROS_CPP_QOS_BEST_EFFORT = 1,
    NROS_CPP_QOS_RELIABILITY_SYSTEM_DEFAULT = 2,
    NROS_CPP_QOS_RELIABILITY_UNKNOWN = 3,
    NROS_CPP_QOS_RELIABILITY_BEST_AVAILABLE = 4,
};
enum nros_cpp_qos_durability_t {
    NROS_CPP_QOS_VOLATILE = 0,
    NROS_CPP_QOS_TRANSIENT_LOCAL = 1,
    NROS_CPP_QOS_DURABILITY_SYSTEM_DEFAULT = 2,
    NROS_CPP_QOS_DURABILITY_UNKNOWN = 3,
    NROS_CPP_QOS_DURABILITY_BEST_AVAILABLE = 4,
};
enum nros_cpp_qos_history_t {
    NROS_CPP_QOS_KEEP_LAST = 0,
    NROS_CPP_QOS_KEEP_ALL = 1,
    NROS_CPP_QOS_HISTORY_SYSTEM_DEFAULT = 2,
    NROS_CPP_QOS_HISTORY_UNKNOWN = 3,
};
enum nros_cpp_qos_liveliness_t {
    NROS_CPP_QOS_LIVELINESS_NONE = 0,
    NROS_CPP_QOS_LIVELINESS_AUTOMATIC = 1,
    NROS_CPP_QOS_LIVELINESS_MANUAL_BY_TOPIC = 2,
    NROS_CPP_QOS_LIVELINESS_MANUAL_BY_NODE = 3,
    NROS_CPP_QOS_LIVELINESS_UNKNOWN = 4,
    NROS_CPP_QOS_LIVELINESS_BEST_AVAILABLE = 5,
};
struct nros_cpp_qos_t {
    enum nros_cpp_qos_reliability_t reliability;
    enum nros_cpp_qos_durability_t durability;
    enum nros_cpp_qos_history_t history;
    enum nros_cpp_qos_liveliness_t liveliness_kind;
    int depth;
    uint32_t deadline_ms;
    uint32_t lifespan_ms;
    uint32_t liveliness_lease_ms;
    uint8_t avoid_ros_namespace_conventions;
    uint8_t tx_express;
};
}
#endif // NROS_CPP_FFI_H

// issue 2054 — upstream's rmw profile struct, under its upstream TAG.
// Declared here, incomplete, so `QoS` below can name it in three member
// declarations; it is DEFINED at the bottom of this file, after `QoS` is
// complete, because its conversion to `QoS` needs the whole class. The tag is
// upstream's `rmw_qos_profile_s`, not `rmw_qos_profile_t`, which is the RMW
// ABI's own struct in `<nros/rmw_entity.h>` — a different type with ABI field
// widths, which this declaration does not touch.
struct rmw_qos_profile_s;

namespace rclcpp {

// -- Policy enums (phase 379 W5) ------------------------------------------
//
// At NAMESPACE scope and public, under rclcpp's names, because that is where a
// ported node looks for them (`rclcpp::ReliabilityPolicy`, `qos.hpp:34-58`).
// Three of these were PRIVATE members of `QoS` until phase-379 W5, which is why
// the getters had to hand back `int` — a public getter cannot name a private
// type. Ledger: `cpp:ReliabilityPolicy` and its three siblings.
//
// The two rclcpp enumerators we used to decline ARRIVED in phase-444 (issue
// 1437), and the reason each was declined is the reason it is now needed:
//
//   * `SystemDefault` — "defer to the middleware". Declined because nothing
//     DEFERS: the backend is linked at build time (RFC-0036). True of a
//     REQUEST, and a `QoS` is no longer only a request — `get_actual_qos()`
//     hands one back, and a backend CAN report that a policy was never stated.
//   * `Unknown` — declined as "a discovery artefact for a policy read off a
//     remote endpoint, and we do no dynamic discovery". It is not that: it is
//     what `rmw_*_get_actual_qos` owes for a policy the backend looked at and
//     cannot determine, on the LOCAL endpoint. `rmw_vtable.h` has always said
//     so, and until issue 1437 no implementation anywhere wrote one.
//
// What is still declined is `rclcpp::SystemDefaultsQoS` — the all-sentinel
// PRESET, whose two RMWs resolve it to depth 1 and depth 42 (issue 0829).
// A value a backend may REPORT is not a profile a caller may REQUEST, and the
// refusal on that class stands (`NROS_RCLCPP_REFUSE_SYSTEM_DEFAULTS_QOS`).
//
// The enumerators are upstream's and SCOPED (phase-483 W1): one
// `SystemDefault` and one `Unknown` per policy, reached as
// `rclcpp::ReliabilityPolicy::Unknown`. The prefixed spellings
// (~~`ReliabilityUnknown`~~, ~~`LivelinessNone`~~) existed only because these were
// unscoped enums, and the bare `rclcpp::Reliable` form went with them.

/// Reliability policy. Matches DDS `RELIABILITY_QOS_POLICY` and
/// `rclcpp::ReliabilityPolicy`.
///
/// `enum class`, as upstream's is (phase-483 W1). These were unscoped while
/// they lived in `nros::`; in `rclcpp::` an unscoped `KeepLast` enumerator
/// would collide with upstream's `rclcpp::KeepLast` profile helper.
enum class ReliabilityPolicy : int {
    Reliable = 0,
    BestEffort = 1,
    /// Nobody stated a reliability policy — the middleware chose. Reportable
    /// by `get_actual_qos()`; not a value to request.
    SystemDefault = 2,
    /// The backend could not determine this policy. An ABSENCE.
    Unknown = 3,
    /// Resolve at creation against the endpoints already discovered on the
    /// topic — upstream's `BestAvailable` (Iron+), issue 1973. A REQUEST:
    /// `get_actual_qos()` reports the concrete policy it resolved to.
    BestAvailable = 4,
};

/// Durability policy. Matches DDS `DURABILITY_QOS_POLICY`.
enum class DurabilityPolicy : int {
    Volatile = 0,
    TransientLocal = 1,
    /// Nobody stated a durability policy — the middleware chose.
    SystemDefault = 2,
    /// The backend could not determine this policy. An ABSENCE.
    Unknown = 3,
    /// Resolve at creation against the discovered graph (issue 1973).
    BestAvailable = 4,
};

/// History policy. Matches DDS `HISTORY_QOS_POLICY`.
enum class HistoryPolicy : int {
    KeepLast = 0,
    KeepAll = 1,
    /// Nobody stated a history policy — the middleware chose.
    SystemDefault = 2,
    /// The backend could not determine this policy. An ABSENCE.
    Unknown = 3,
};

/// Liveliness policy kind. Matches DDS `LIVELINESS_QOS_POLICY`.
///
/// `SystemDefault` is discriminant 0, the value rmw gives
/// `RMW_QOS_POLICY_LIVELINESS_SYSTEM_DEFAULT`; this enumerator was spelled
/// ~~`LivelinessNone`~~ before phase-483. `ManualByNode` is ours-only — rmw
/// deprecated it, so rclcpp's `LivelinessPolicy` does not list it.
enum class LivelinessPolicy : int {
    SystemDefault = 0,
    Automatic = 1,
    ManualByTopic = 2,
    ManualByNode = 3,
    /// The backend could not determine this policy. An ABSENCE, reportable by
    /// `get_actual_qos()` and never a value to request.
    Unknown = 4,
    /// Resolve at creation against the discovered graph (issue 1973).
    BestAvailable = 5,
};

class QoS;
struct QoSInitialization;

namespace detail {

/// Milliseconds one QoS window is worth, at the `Duration` → C-ABI boundary.
///
/// The C ABI is `uint32_t` milliseconds (`nros_cpp_qos_t.deadline_ms` and
/// siblings) and `0` there means INFINITE, so truncation is not a rounding
/// question — a 500 µs deadline truncated to `0` would silently mean "no
/// deadline at all". So:
///
///   * `<= 0` → `0`. A non-positive window is the "unset" spelling
///     (`Duration()` / `Duration::zero()`), and `0` is the ABI's infinity.
///   * otherwise ROUND UP to the next whole millisecond. A sub-millisecond
///     window becomes 1 ms rather than infinity, and rounding up is the
///     lenient direction for all three policies (a longer deadline, lifespan
///     or lease never turns a working profile into a violated one).
///   * a window that would land at or above `UINT32_MAX - 1` ms (~49.7 days)
///     → `UINT32_MAX`, saturating to infinity rather than wrapping into a
///     short window. The bound starts one below `UINT32_MAX` because that
///     value is the best-available sentinel (issue 1973), which only
///     `qos_best_available_duration()` reaches.
constexpr uint32_t qos_window_ms(const Duration& d) {
    return d.nanoseconds() <= 0 ? 0u
           : d.nanoseconds() == INT64_MAX - 1
               ? UINT32_MAX - 1u
               : (d.nanoseconds() >= static_cast<int64_t>(UINT32_MAX - 1u) * 1000000 - 999999
                      ? UINT32_MAX
                      : static_cast<uint32_t>((d.nanoseconds() + 999999) / 1000000));
}

/// The best-available window — issue 1973.
///
/// Upstream spells it `RMW_QOS_DEADLINE_BEST_AVAILABLE` /
/// `RMW_QOS_LIVELINESS_LEASE_DURATION_BEST_AVAILABLE`, both
/// `RMW_DURATION_INFINITE - 1` = `INT64_MAX - 1` ns. As a `Duration` it is that
/// same value, and `qos_window_ms` lowers it — and only it — onto the C ABI's
/// `NROS_RMW_DURATION_BEST_AVAILABLE_MS` (`UINT32_MAX - 1`).
constexpr Duration qos_best_available_duration() {
    return Duration::from_nanoseconds(INT64_MAX - 1);
}

/// The inverse: the C ABI's milliseconds as a `Duration`. Exact, and exact in
/// both directions — `qos_window_ms(qos_window_duration(ms)) == ms` — the
/// best-available sentinel included.
constexpr Duration qos_window_duration(uint32_t ms) {
    return ms == UINT32_MAX - 1u ? qos_best_available_duration()
                                 : Duration::from_nanoseconds(static_cast<int64_t>(ms) * 1000000);
}

/// issue 1437 — declared here, defined below `QoS`, and named as a friend of
/// it. See the definition for what it is for and why it is the one function
/// in this header that writes `QoS`'s members.
constexpr QoS qos_from_ffi(const nros_cpp_qos_t& f);

} // namespace detail

/// QoS profile for publishers and subscriptions.
///
/// Mirrors rclcpp::QoS with chainable setters and predefined profiles.
/// All methods are constexpr — profiles can be computed at compile time.
///
/// Phase 108.B.7 — extended with full DDS QoS surface (deadline,
/// lifespan, liveliness kind + lease, avoid_ros_namespace_conventions).
/// Backends advertise per-policy support; entities created with a
/// profile the active backend can't honour return
/// ~~`NROS_CPP_RET_INCOMPATIBLE_QOS`~~ synchronously at create time.
///
/// Phase 379 W5 — the policy enums moved to namespace scope, the getters
/// return them instead of `int`, and the three time windows take and return
/// `rclcpp::Duration` instead of carrying an `_ms` suffix. The old spellings were
/// deleted by phase-482 W6.
class QoS {
  public:
    /// Default QoS: reliable, volatile, keep-last(10), automatic
    /// liveliness, no deadline / lifespan / lease.
    constexpr QoS()
        : reliability_(ReliabilityPolicy::Reliable), durability_(DurabilityPolicy::Volatile),
          history_(HistoryPolicy::KeepLast), liveliness_(::rclcpp::LivelinessPolicy::SystemDefault),
          depth_(10), deadline_ms_(0), lifespan_ms_(0), liveliness_lease_ms_(0),
          avoid_ros_namespace_conventions_(0), tx_express_(0) {}

    /// rclcpp's depth ctor: `QoS(1)` == default profile with keep-last(1).
    /// IMPLICIT, as upstream's `QoS(size_t history_depth)` is, so a bare depth
    /// converts wherever a profile is expected (`create_publisher<M>(t, 10)`).
    /// Before phase-483 this was an explicit `int` ctor on `nros::QoS` and a
    /// derived `rclcpp::QoS` added the implicit one; the two are one class now.
    // NOLINTNEXTLINE(google-explicit-constructor)
    constexpr QoS(::size_t depth) : QoS() { depth_ = static_cast<int>(depth); }

    /// Upstream's `QoS(QoSInitialization, rmw_qos_profile_t)` and its
    /// one-argument form, which starts from `rmw_qos_profile_default` (issue
    /// 2054): the profile's policies, with the history and depth `init` states.
    /// `rclcpp::QoS(rclcpp::QoSInitialization::from_rmw(p), p)` is the ported
    /// spelling. Defined beside `rmw_qos_profile_t` at the bottom of this
    /// file.
    explicit constexpr QoS(const QoSInitialization& init);
    constexpr QoS(const QoSInitialization& init, const ::rmw_qos_profile_s& profile);

    // -- Chainable setters (match rclcpp fluent API) --

    /// Set reliability to `RELIABLE` (acked transport, retransmits on loss).
    constexpr QoS& reliable() {
        reliability_ = ReliabilityPolicy::Reliable;
        return *this;
    }

    /// Set reliability to `BEST_EFFORT` (fire-and-forget; default for sensors).
    constexpr QoS& best_effort() {
        reliability_ = ReliabilityPolicy::BestEffort;
        return *this;
    }

    /// Set durability to `TRANSIENT_LOCAL` — late joiners get the last value.
    constexpr QoS& transient_local() {
        durability_ = DurabilityPolicy::TransientLocal;
        return *this;
    }

    /// Set durability to `VOLATILE` — late joiners get nothing (default).
    constexpr QoS& durability_volatile() {
        durability_ = DurabilityPolicy::Volatile;
        return *this;
    }

    /// Set reliability to `BEST_AVAILABLE` — `rclcpp::QoS::reliability_best_available`
    /// (issue 1973). Resolved when the entity is created: a subscription takes
    /// RELIABLE only if every publisher already discovered offers it, a
    /// publisher always takes RELIABLE. `get_actual_qos()` reports the result.
    constexpr QoS& reliability_best_available() {
        reliability_ = ReliabilityPolicy::BestAvailable;
        return *this;
    }

    /// Set durability to `BEST_AVAILABLE` — `rclcpp::QoS::durability_best_available`
    /// (issue 1973). Same creation-time resolution as
    /// `reliability_best_available()`, over TRANSIENT_LOCAL / VOLATILE.
    constexpr QoS& durability_best_available() {
        durability_ = DurabilityPolicy::BestAvailable;
        return *this;
    }

    /// Use `KEEP_LAST` history with the given depth.
    /// @param depth maximum number of messages buffered per entity.
    constexpr QoS& keep_last(int depth) {
        history_ = HistoryPolicy::KeepLast;
        depth_ = depth;
        return *this;
    }

    /// Use `KEEP_ALL` history (bounded by transport).
    constexpr QoS& keep_all() {
        history_ = HistoryPolicy::KeepAll;
        return *this;
    }

    /// Subscriber max-inter-arrival / publisher offered-rate.
    /// A zero or negative duration = infinite (no deadline check).
    ///
    /// The C ABI underneath is `uint32_t` milliseconds, so a sub-millisecond
    /// window ROUNDS UP to 1 ms — it never truncates to `0`, which the ABI
    /// reads as infinity. Above ~49.7 days it saturates. See
    /// `detail::qos_window_ms`.
    constexpr QoS& deadline(const Duration& d) {
        deadline_ms_ = detail::qos_window_ms(d);
        return *this;
    }

    /// Sample expiry. A zero or negative duration = infinite.
    /// Same millisecond boundary as `deadline()`.
    constexpr QoS& lifespan(const Duration& d) {
        lifespan_ms_ = detail::qos_window_ms(d);
        return *this;
    }

    /// Liveliness kind. Pair with `liveliness_lease_duration()` for
    /// `MANUAL_BY_TOPIC` / `MANUAL_BY_NODE`. AUTOMATIC is the default.
    constexpr QoS& liveliness(LivelinessPolicy kind) {
        liveliness_ = kind;
        return *this;
    }

    /// Liveliness lease duration. A zero or negative duration = infinite.
    /// Same millisecond boundary as `deadline()`.
    constexpr QoS& liveliness_lease_duration(const Duration& d) {
        liveliness_lease_ms_ = detail::qos_window_ms(d);
        return *this;
    }

    /// Skip the ROS `/rt/` topic-name prefix. Off by default; enable
    /// when interoperating with non-ROS DDS endpoints.
    constexpr QoS& avoid_ros_namespace_conventions(bool on) {
        avoid_ros_namespace_conventions_ = on ? 1 : 0;
        return *this;
    }

    /// Phase 282 (#145) — mark this publisher "express": its samples bypass
    /// transport tx batching (sent immediately even when the batching knob
    /// is on). A transport hint for control-tier / latency-sensitive topics;
    /// ignored on subscriptions and by backends without batching.
    constexpr QoS& tx_express(bool on) {
        tx_express_ = on ? 1 : 0;
        return *this;
    }

    // -- Predefined profiles (match rclcpp named constructors) --
    //
    // phase-428 W10 — DECLARED here, DEFINED below from `rclcpp::detail::qos_table`,
    // which is the one place this header spells a profile's fields. Until W10
    // these three carried their own literals AND the `rclcpp::*QoS` classes at
    // the bottom of the file carried a second, independent set: `sensor_data`
    // and `services_default` were each written twice inside ONE header, with
    // nothing asserting they agreed. That is the hand-mirror class
    // (issues 0088 / 0160 / 0245) at file scope.

    /// Default profile: `RELIABLE` + `VOLATILE` + `KEEP_LAST(10)`.
    static constexpr QoS default_profile();

    /// Sensor-data profile: `BEST_EFFORT` + `VOLATILE` + `KEEP_LAST(5)`.
    static constexpr QoS sensor_data();

    /// Services profile: `RELIABLE` + `VOLATILE` + `KEEP_LAST(10)`.
    static constexpr QoS services();

    // -- Accessors --

    /// The reliability policy.
    constexpr ReliabilityPolicy reliability() const { return reliability_; }
    /// The durability policy.
    constexpr DurabilityPolicy durability() const { return durability_; }
    /// The history policy.
    constexpr HistoryPolicy history() const { return history_; }
    /// The liveliness policy kind.
    constexpr LivelinessPolicy liveliness() const { return liveliness_; }
    /// Configured queue depth (only meaningful for `KEEP_LAST`).
    constexpr int depth() const { return depth_; }
    /// The deadline window. `Duration()` (zero) = infinite.
    constexpr Duration deadline() const { return detail::qos_window_duration(deadline_ms_); }
    /// The lifespan window. `Duration()` (zero) = infinite.
    constexpr Duration lifespan() const { return detail::qos_window_duration(lifespan_ms_); }
    /// The liveliness lease window. `Duration()` (zero) = infinite.
    constexpr Duration liveliness_lease_duration() const {
        return detail::qos_window_duration(liveliness_lease_ms_);
    }
    /// Whether to skip the `/rt/` ROS topic-name prefix.
    constexpr bool avoid_ros_namespace_conventions() const {
        return avoid_ros_namespace_conventions_ != 0;
    }
    /// Whether this publisher's samples bypass transport tx batching.
    constexpr bool tx_express() const { return tx_express_ != 0; }

    /// Upstream's `get_rmw_qos_profile()` — this profile as an
    /// `rmw_qos_profile_t` (issue 2054). Returns by VALUE where upstream hands
    /// back a reference to its stored struct: `QoS` stores the C ABI's fields,
    /// not upstream's, so there is no struct to refer to, and a copy is what
    /// every ported caller (`auto p = qos.get_rmw_qos_profile();`) takes anyway.
    constexpr ::rmw_qos_profile_s get_rmw_qos_profile() const;

  private:
    // issue 1437 — the ONE function allowed to write these directly.
    //
    // `qos_from_ffi` builds the profile a backend GRANTED, and a grant can
    // state `ReliabilityPolicy::Unknown` / `HistoryPolicy::SystemDefault` / ... which NO
    // public setter can produce: the setters are `reliable()`,
    // `best_effort()`, `keep_last(n)` — the vocabulary of a REQUEST, and a
    // request may not carry a sentinel. Adding enum-taking setters to reach
    // them would hand every caller a way to ask for one, which is the refusal
    // `NROS_RCLCPP_REFUSE_SYSTEM_DEFAULTS_QOS` exists to keep. A friend for
    // the read-back direction alone keeps both properties.
    friend constexpr QoS detail::qos_from_ffi(const nros_cpp_qos_t& f);

    // The private members keep their `_ms_` spelling: they hold exactly what
    // the C ABI carries (`uint32_t` milliseconds), and renaming them would say
    // the storage changed when only the accessors did.
    ReliabilityPolicy reliability_;
    DurabilityPolicy durability_;
    HistoryPolicy history_;
    LivelinessPolicy liveliness_;
    int depth_;
    uint32_t deadline_ms_;
    uint32_t lifespan_ms_;
    uint32_t liveliness_lease_ms_;
    uint8_t avoid_ros_namespace_conventions_;
    uint8_t tx_express_;
};

// ============================================================================
// phase-417 G6 — profile comparison. `rclcpp::operator==(const QoS&, const QoS&)`
// ============================================================================
//
// FREE functions, as upstream's are, and written from the PUBLIC accessors —
// every one of the ten members has one, so no `friend` is needed and the
// comparison stays a thing a reader can check against the accessor list.
//
// TEN hand-written comparisons rather than `= default`. `operator==() =
// default` is C++20 and this surface's floor is `cxx_std_17`
// (`packages/api/nros-cpp/CMakeLists.txt` — `target_compile_features(
// nros-cpp-headers INTERFACE cxx_std_17)`), so the defaulted form would not
// compile for the targets this header exists to serve. Still `constexpr`,
// still no allocator, still no exceptions.
//
// Rust's `QoSProfile` derives `PartialEq, Eq` and has had this for nothing
// since it existed; the ledger row `cpp:operator==` recorded the asymmetry as
// having no platform reason, and it did not.
//
// The three time windows are compared through `Duration::nanoseconds()`: the
// storage is milliseconds and the conversion is exact in both directions
// (`detail::qos_window_duration`).

/// `true` when both profiles state the same value for every policy.
///
/// A WHOLE-PROFILE comparison, which is upstream's: two profiles that would be
/// granted the same thing by a backend still compare unequal if they were
/// written differently, because this compares what was REQUESTED.
constexpr bool operator==(const QoS& left, const QoS& right) {
    return left.reliability() == right.reliability() && left.durability() == right.durability() &&
           left.history() == right.history() && left.liveliness() == right.liveliness() &&
           left.depth() == right.depth() &&
           left.deadline().nanoseconds() == right.deadline().nanoseconds() &&
           left.lifespan().nanoseconds() == right.lifespan().nanoseconds() &&
           left.liveliness_lease_duration().nanoseconds() ==
               right.liveliness_lease_duration().nanoseconds() &&
           left.avoid_ros_namespace_conventions() == right.avoid_ros_namespace_conventions() &&
           left.tx_express() == right.tx_express();
}

/// The negation of `operator==`, and written as its negation so the two cannot
/// disagree.
constexpr bool operator!=(const QoS& left, const QoS& right) {
    return !(left == right);
}

// ============================================================================
// phase-417 G7 — naming a QoS policy
// ============================================================================

/// One QoS policy's name — `rclcpp::qos_policy_kind_to_cstr`.
///
/// @param policy ONE `NROS_RMW_QOS_POLICY_*` bit, as declared in
///        `<nros/rmw_entity.h>` and as a backend advertises through the
///        vtable's `supported_qos_policies` slot.
/// @return `"RELIABILITY"`, `"DEADLINE"`, … — borrowed, static lifetime, never
///         freed — or `nullptr` when @p policy is not exactly one policy bit.
///
/// **What this is FOR.** An `NROS_CPP_RET_NOT_ALLOWED` from a `create_*` means
/// the backend declined a policy, and the return code alone does not say which
/// — while `rmw_vtable.h`'s own `supported_qos_policies` block told the reader
/// those creates fail "naming a policy". The refusal is logged with this name
/// now (`nros_node`'s `validate_qos_or_report`), and this is how a C++ caller
/// spells the same policy in its own message.
///
/// **Two deliberate differences from rclcpp**, both from the same constraint:
/// upstream takes a `rclcpp::QosPolicyKind` enum and returns a `const char*`
/// over `std::string`-free storage; ours takes the MASK BIT, because the
/// bitmask is the vocabulary this tree has — one set of twelve values shared by
/// `QoSPolicyMask` in Rust and `NROS_RMW_QOS_POLICY_*` in C, held equal by
/// `check-qos-mask-derivation`. And upstream returns `"Invalid"` for its
/// invalid enumerator where we return `nullptr`: we have no invalid
/// enumerator, and "no policy was identified" is a real answer a caller must
/// be able to tell apart from a policy that was.
///
/// The bytes are `rclc_qos_policy_kind_to_cstr`'s, which are
/// `nros_rmw::QoSPolicyMask::NAMED`'s. This header declares the C entry point
/// rather than including `<rclc/detail/nros_generated.h>` — `qos.hpp` is deliberately
/// self-contained (see issue 0112 at the top of this file) — and a signature
/// drift cannot hide: `nros.hpp` pulls both this header and `rclc/types.h`,
/// which reaches the generated declarations, so every C++ example is a
/// translation unit holding both spellings and the compiler compares them.
/// The declaration is spelled EXACTLY as `nros_generated.h` spells it,
/// `NROS_PUBLIC` included. A C-linkage function declared in two scopes is one
/// entity, so the two must agree — and a visibility attribute that appears on
/// only one of them is "ignored because it conflicts with previous
/// declaration", a warning that becomes an error on this tree's `-Werror`
/// lanes and whose text points at neither file's real problem.
extern "C" NROS_PUBLIC const char* rclc_qos_policy_kind_to_cstr(uint32_t policy);

/// `rclcpp::qos_policy_kind_to_cstr` — see the entry point above, which it
/// forwards to unchanged.
inline const char* qos_policy_kind_to_cstr(uint32_t policy) {
    return rclc_qos_policy_kind_to_cstr(policy);
}

namespace detail {

// --- THE C++ QoS profile table (phase-428 W10) -------------------------------
//
// Every named profile the C++ surface offers is spelled HERE and nowhere else.
// `rclcpp::QoS`'s three named statics and all six concrete `rclcpp::*QoS` classes
// are defined by calling into this block; none of them restates a policy.
//
// Why a table and not a hand copy per class: W10 exists because upstream's 7
// profiles were transcribed in four places across the repo — this header,
// `nros-rmw/src/traits.rs`, `nros/src/lib.rs` and `nros-rmw-abi/.../
// rmw_entity.h` — about 26 constants for a 7-row table, with no gate binding
// them. `QOS_PROFILE_PARAMETER_EVENTS` is the proof: `KeepAll, 0` in Rust under
// a comment claiming it matched `rmw_qos_profile_parameter_events`, which is
// `KEEP_LAST, 1000`. This header had the same profile right, which is exactly
// what an ungated mirror looks like until the day it is not.
//
// This block cannot CONSUME the cross-language SSoT. The C spellings in
// `rmw_entity.h` are `((rmw_qos_profile_t){...})` compound literals, which are
// C99 and not standard C++, over a struct whose field types differ from
// `nros_cpp_qos_t`'s; and `qos.hpp` is deliberately self-contained and
// freestanding (see issue 0112 at the top of this file), so it cannot take a
// dependency on a build-generated header either. What the table DOES buy is
// that the C++ side has ONE row per profile for a cross-language gate to read,
// instead of nine constructor bodies to parse.
//
// phase-454 W1 collected on that. `scripts/check-qos-profile-ssot.py` now
// EVALUATES these rows — the ctor supplies the base profile and each chained
// call applies its one effect — and compares every field against
// `QoSProfile::QOS_PROFILE_*`. So the table is bound after all; what it is not
// is DERIVED, which is the weaker of the two and the strongest available to a
// freestanding header. A `.setter()` the gate does not model is a hard failure
// there, not a skipped field.
//
// Values read 2026-09-05 against ROS 2 Humble as installed:
//   /opt/ros/humble/include/rmw/rmw/qos_profiles.h              :25,38,51,64,77,90
//   /opt/ros/humble/include/rcl/rcl/logging_rosout.h            :36
//   /opt/ros/humble/include/rclcpp/rclcpp/qos.hpp               :371-500
//
// Three fields are identical in EVERY upstream profile and are therefore folded
// into `QoS`'s default constructor rather than spelled per row:
//   * `avoid_ros_namespace_conventions` is `false` upstream, `0` here.
//   * liveliness is `RMW_QOS_POLICY_LIVELINESS_SYSTEM_DEFAULT` upstream, and
//     `rclcpp::QoS` now defaults to `LivelinessPolicy::SystemDefault`, which is our spelling of
//     that sentinel. It defaulted to ~~`LivelinessAutomatic`~~ — "what every
//     reference RMW folds that sentinel to" — under a
//     `nros-qos-mirror-deviation` record whose own reason was that this
//     surface and `nros-c`'s "move together with the C ABI or not at all".
//     They moved together (issue 1329): this constructor, `nros-c`'s
//     `NROS_QOS_*` statics and (until phase-492 W4) `component.h`'s
//     ~~`rclc_c_qos_default()`~~.
//
//     The forcing argument is issue 1329's per-backend QoS mask. XRCE honours
//     no liveliness, so an AUTOMATIC default would have made
//     `required_policies()` demand `LIVELINESS_AUTOMATIC` and refuse every C++
//     default profile on that backend — over a policy upstream leaves unset
//     and the application never asked for. A caller that wants automatic
//     liveliness states `.liveliness(rclcpp::LivelinessPolicy::Automatic)`.
//
//   * deadline and the liveliness lease are `RMW_QOS_*_DEFAULT` (infinite)
//     upstream and `0` here, which `qos_window_ms` above documents as infinite.
//
// `rmw_qos_profile_system_default` has NO row: every one of its fields is a
// sentinel, so no value could be right. `rclcpp::SystemDefaultsQoS` refuses at
// compile time instead — see the class below.
struct qos_table {
    /// `rmw_qos_profile_default` — KEEP_LAST(10), RELIABLE, VOLATILE.
    ///
    /// Spelled by `QoS`'s default constructor's member-init list, which IS this
    /// profile; naming it here would be the second copy this table removes.
    static constexpr QoS default_profile() { return QoS(); }

    /// `rmw_qos_profile_sensor_data` — KEEP_LAST(5), BEST_EFFORT, VOLATILE.
    static constexpr QoS sensor_data() { return QoS().best_effort().keep_last(5); }

    /// `rmw_qos_profile_services_default` — KEEP_LAST(10), RELIABLE, VOLATILE.
    ///
    /// The depth is stated rather than inherited from `QoS()`: upstream states
    /// it, and a row that leaves a field to a default cannot be compared to
    /// upstream's row field by field.
    static constexpr QoS services_default() { return QoS().reliable().keep_last(10); }

    /// `rmw_qos_profile_parameters` — KEEP_LAST(**1000**), RELIABLE, VOLATILE.
    static constexpr QoS parameters() { return QoS().reliable().keep_last(1000); }

    /// `rmw_qos_profile_parameter_events` — KEEP_LAST(1000), RELIABLE, VOLATILE.
    ///
    /// Same shape as `parameters()` upstream and still its own row: they are two
    /// upstream constants that happen to agree today, and collapsing them would
    /// make a future divergence invisible.
    static constexpr QoS parameter_events() { return QoS().reliable().keep_last(1000); }

    /// `rcl_qos_profile_rosout_default` — KEEP_LAST(1000), RELIABLE,
    /// TRANSIENT_LOCAL, lifespan {10, 0}.
    static constexpr QoS rosout() {
        return QoS().reliable().transient_local().keep_last(1000).lifespan(
            ::rclcpp::Duration::from_nanoseconds(10000000000LL));
    }

    /// `rclcpp::ClockQoS` — KEEP_LAST(1), BEST_EFFORT, VOLATILE.
    ///
    /// Upstream builds it from `rmw_qos_profile_sensor_data` with `KeepLast(1)`
    /// (`rclcpp/qos.hpp:371-377`), which is why the depth differs from
    /// `sensor_data()`'s 5 and the reliability does not.
    static constexpr QoS clock() { return QoS().best_effort().keep_last(1); }

    /// `rmw_qos_profile_best_available` (Iron+) — KEEP_LAST(10), and every
    /// policy that can be best-available set so: reliability, durability,
    /// liveliness, deadline, liveliness lease (issue 1973).
    static constexpr QoS best_available() {
        return QoS()
            .reliability_best_available()
            .durability_best_available()
            .keep_last(10)
            .liveliness(::rclcpp::LivelinessPolicy::BestAvailable)
            .deadline(::rclcpp::detail::qos_best_available_duration())
            .liveliness_lease_duration(::rclcpp::detail::qos_best_available_duration());
    }
};

/// `QoS` → the by-value C ABI record, in ONE place.
///
/// Ten call sites across `publisher.hpp` / `subscription.hpp` / `service.hpp` /
/// `client.hpp` / `action_{server,client}.hpp` / `component.hpp` open-coded
/// this ten-field copy. A field appended to `nros_cpp_qos_t` then has ten
/// places to be missed, which is the hand-mirror class issue 0160 records;
/// phase-379 W5 folded them into this one function while renaming the
/// accessors they called.
/// `constexpr` so a fixed profile's marshalled form is checkable at compile
/// time (`qos_policy_accessors.cpp` static_asserts the millisecond fields);
/// value-initialised because C++14 forbids an uninitialised local in a
/// `constexpr` function, and zeroing ten fields we then overwrite costs
/// nothing.
constexpr nros_cpp_qos_t qos_to_ffi(const QoS& qos) {
    nros_cpp_qos_t f{};
    f.reliability = static_cast<nros_cpp_qos_reliability_t>(qos.reliability());
    f.durability = static_cast<nros_cpp_qos_durability_t>(qos.durability());
    f.history = static_cast<nros_cpp_qos_history_t>(qos.history());
    f.liveliness_kind = static_cast<nros_cpp_qos_liveliness_t>(qos.liveliness());
    f.depth = qos.depth();
    // The FFI FIELDS are milliseconds and keep their `_ms` names; only the
    // accessors changed. `qos_window_ms` is the exact inverse of the
    // `Duration` the getter built, so this round-trip loses nothing.
    f.deadline_ms = qos_window_ms(qos.deadline());
    f.lifespan_ms = qos_window_ms(qos.lifespan());
    f.liveliness_lease_ms = qos_window_ms(qos.liveliness_lease_duration());
    f.avoid_ros_namespace_conventions = qos.avoid_ros_namespace_conventions() ? 1 : 0;
    f.tx_express = qos.tx_express() ? 1 : 0;
    return f;
}

/// The all-absent profile — issue 1437.
///
/// Every policy `UNKNOWN`: "there is nothing here that can tell you". What a
/// `get_actual_qos()` returns when the handle it was asked of is not live, so
/// the answer is never a plausible-looking concrete profile the caller would
/// then compare against its request and find agreeable. `QOS_PROFILE_UNKNOWN`
/// in Rust, `NROS_RMW_QOS_PROFILE_UNKNOWN` across the RMW ABI; this is the
/// same statement in this header's vocabulary.
///
/// Written from the `rclcpp::` enumerators and `static_cast` across, NOT from
/// the `NROS_CPP_QOS_*` ones: this header defines those itself only when
/// `nros_cpp_ffi.h` was NOT included first, and when it WAS, cbindgen has
/// spelled them `nros_cpp_qos_reliability_t_NROS_CPP_QOS_RELIABILITY_UNKNOWN`
/// — type-name-prefixed. So a bare enumerator here compiles in one include
/// order and not the other, which is how the first version of this function
/// broke the C++ surface extraction while every C++ test still passed.
constexpr QoS qos_all_unknown() {
    nros_cpp_qos_t f{};
    f.reliability = static_cast<nros_cpp_qos_reliability_t>(ReliabilityPolicy::Unknown);
    f.durability = static_cast<nros_cpp_qos_durability_t>(DurabilityPolicy::Unknown);
    f.history = static_cast<nros_cpp_qos_history_t>(HistoryPolicy::Unknown);
    f.liveliness_kind = static_cast<nros_cpp_qos_liveliness_t>(LivelinessPolicy::Unknown);
    f.depth = 0;
    return qos_from_ffi(f);
}

/// The inverse of `qos_to_ffi` — issue 1437.
///
/// Used by the `get_actual_qos()` family, which is the only direction that
/// produces a profile the CALLER did not write. The four `static_cast`s are
/// the same value-for-value mirror `qos_to_ffi` relies on in the other
/// direction: `rclcpp::ReliabilityPolicy` and `nros_cpp_qos_reliability_t` agree
/// enumerator for enumerator, sentinels included, and the static_asserts in
/// `tests/compile/qos_policy_accessors.cpp` measure that rather than trusting
/// it. Neither vocabulary is the RMW ABI's, whose numbering differs from both
/// in all four enums — nothing here may be cast to an `NROS_RMW_*` value.
///
/// Writes `QoS`'s members directly — see the `friend` declaration on the
/// class for why this one function may and no setter can. The three windows
/// are stored as the milliseconds the ABI carries, so the round-trip through
/// `qos_to_ffi` is exact.
constexpr QoS qos_from_ffi(const nros_cpp_qos_t& f) {
    QoS q;
    q.reliability_ = static_cast<ReliabilityPolicy>(f.reliability);
    q.durability_ = static_cast<DurabilityPolicy>(f.durability);
    q.history_ = static_cast<HistoryPolicy>(f.history);
    q.liveliness_ = static_cast<LivelinessPolicy>(f.liveliness_kind);
    q.depth_ = f.depth;
    q.deadline_ms_ = f.deadline_ms;
    q.lifespan_ms_ = f.lifespan_ms;
    q.liveliness_lease_ms_ = f.liveliness_lease_ms;
    q.avoid_ros_namespace_conventions_ = f.avoid_ros_namespace_conventions;
    q.tx_express_ = f.tx_express;
    return q;
}

} // namespace detail

// phase-428 W10 — the three named statics declared on `QoS`, defined from the
// one table. Out of line because the table needs a COMPLETE `QoS` and these are
// members of it; `constexpr` implies `inline`, so this is still header-only and
// still usable in the `static_assert`s and default arguments that call it.
constexpr QoS QoS::default_profile() {
    return detail::qos_table::default_profile();
}
constexpr QoS QoS::sensor_data() {
    return detail::qos_table::sensor_data();
}
constexpr QoS QoS::services() {
    return detail::qos_table::services_default();
}

} // namespace rclcpp

// ============================================================================
// rclcpp:: — the ROS 2 spelling of the QoS surface (RFC-0089 stage 6, step A)
// ============================================================================
//
// Moved here from `nros/rclcpp_compat.hpp`, which no longer exists as a
// separate surface: RFC-0089 §"Naming: replace, with alias as the migration
// step" makes the ROS 2 spelling a first-class name declared by the API header
// that owns the concept. Step B, retiring the in-tree `nros::QoS` spelling,
// landed in phase-483 W1.
//
// Everything below is freestanding-safe — `constexpr` classes over `rclcpp::QoS`
// and one `<type_traits>` predicate — so a `no_std` C++ build gets the ROS 2
// QoS vocabulary too, which the hosted-STL shim could never offer it.

namespace rclcpp {

namespace detail {

/// Is this argument a QoS profile rather than a callback?
///
/// The refusing `create_service(name, F, qos)` overload on `rclcpp::Node` must
/// not swallow `create_service<S>("name", rclcpp::ServicesQoS())`. Deducing `F`
/// is an exact match while binding `const rclcpp::QoS&` needs a derived-to-base
/// conversion, so without this guard the callback template WINS and a perfectly
/// good poll-style call fails with the shared_ptr-callback diagnostic — a
/// refusal firing on something it does not describe, which is worse than no
/// refusal. Caught by the positive probe on its first compile.
/// Written WITHOUT `<type_traits>`, and the two failures that forced it are
/// different failures:
///
/// * On the Zephyr arm-none-eabi toolchain `__has_include(<type_traits>)` is
///   TRUE and the header is INCOMPLETE -- `enable_if` and `is_convertible` are
///   there, `is_base_of` and `decay` are not. `__has_include` answers "does the
///   header exist", which is the wrong question for a header a freestanding
///   toolchain ships hollowed out. (Issue 0112's class; step A verified against
///   a SYNTHESISED minimal libcpp that happened to be more complete than the
///   real one, so the lane stayed green.)
/// * Gating the include on `NROS_CPP_STD` instead fixed Zephyr and broke the
///   freestanding-syntax lane, because the two predicates do not partition the
///   lanes the way the names suggest. The `rclcpp::` block in `nros.hpp` is
///   NOT behind `NROS_CPP_STD` -- step A declared it unconditionally, and it
///   hands out `std::shared_ptr` and `std::string` -- so the freestanding lane
///   compiles `rclcpp::Node` against a SYNTHESISED minimal libcpp while
///   defining no opt-in at all. There the header exists and the macro does not;
///   on Zephyr the macro is available and the header is hollow. Neither
///   predicate is true in both lanes.
///
/// So the predicate depends on neither: a derived-to-base pointer conversion
/// under overload resolution answers `is_base_of` with nothing but the core
/// language, and the partial specialisations answer `decay` for every form a
/// by-value template parameter can take.
template <typename F> struct qos_arg_strip {
    typedef F type;
};
template <typename F> struct qos_arg_strip<F&> {
    typedef F type;
};
template <typename F> struct qos_arg_strip<const F&> {
    typedef F type;
};
template <typename F> struct qos_arg_strip<const F> {
    typedef F type;
};

template <typename F> class is_qos_arg {
    typedef char yes_t[1];
    typedef char no_t[2];
    // Declared, never defined: both calls live in `sizeof`, which does not
    // evaluate its operand.
    static yes_t& probe(const ::rclcpp::QoS*);
    // issue 2054 — an `rmw_qos_profile_t` is a profile argument too, as
    // `create_service<S>(name, rmw_qos_profile_services_default)` passes it.
    // Incomplete here, which a pointer probe does not mind.
    static yes_t& probe(const ::rmw_qos_profile_s*);
    static no_t& probe(...);
    static typename qos_arg_strip<F>::type* arg();

  public:
    static const bool value = sizeof(probe(arg())) == sizeof(yes_t);
};

} // namespace detail

// --- Named QoS profiles (RFC-0089 W3.f; bound to one table, phase-428 W10) ---
//
// Each class below is a NAME for a row of `rclcpp::detail::qos_table`, and states
// no policy of its own. Before W10 each carried its own literals, so
// `SensorDataQoS` and `ServicesQoS` restated what `rclcpp::QoS::sensor_data()` and
// `rclcpp::QoS::services()` already said, 400 lines up in the same header — the
// shape upstream itself avoids: rclcpp's `SensorDataQoS` is
// `QoSInitialization::from_rmw(rmw_qos_profile_sensor_data)`, a REFERENCE to the
// rmw constant, never a re-transcription of it (`rclcpp/qos.hpp:391-399`).
//
// The reason a name-only class is still worth having is unchanged: two of these
// shipped WRONG, which is why the values are pinned by a test rather than by
// review.
//
//   * `ParametersQoS()` returned `QoS(10)` where `rmw_qos_profile_parameters`
//     is KEEP_LAST/**1000** — a hundredfold history difference under a name
//     that claims to be the ROS 2 profile, costing samples under load with
//     nothing to read.
//   * `SystemDefaultsQoS()` returned `QoS(10)`, which is
//     `rmw_qos_profile_default` — a DIFFERENT upstream profile. It is now
//     REFUSE-LOUD; see the class below for why no value can be right.
//
// The upstream sources, and the three fields folded into `QoS()` rather than
// spelled per row, are documented on `qos_table` itself — one place, because
// that is the whole point of this work item.
//
// These are CLASSES, as upstream's are, so `rclcpp::SensorDataQoS{}` — the
// brace form — compiles alongside `rclcpp::SensorDataQoS()`. They are
// `constexpr`, so the table-driven check lives in `static_assert`s
// (`tests/compile/ros2_api_adoption_stage2.cpp`) rather than in a runtime test
// no embedded lane runs.

/// `rmw_qos_profile_sensor_data` — KEEP_LAST(5), BEST_EFFORT, VOLATILE. ADOPT.
class SensorDataQoS : public QoS {
  public:
    constexpr SensorDataQoS() : QoS(::rclcpp::detail::qos_table::sensor_data()) {}
};

/// `rmw_qos_profile_services_default` — KEEP_LAST(10), RELIABLE, VOLATILE.
/// ADOPT. (Verified rather than assumed: this one was already right, but it
/// was right by way of `QoS(10)`, which states the depth and leaves the
/// reliability to the default. The table row states both.)
class ServicesQoS : public QoS {
  public:
    constexpr ServicesQoS() : QoS(::rclcpp::detail::qos_table::services_default()) {}
};

/// `rmw_qos_profile_parameters` — KEEP_LAST(**1000**), RELIABLE, VOLATILE.
/// ADOPT, at the corrected depth. This is one of RFC-0089's two live
/// inversions: it returned `QoS(10)`.
class ParametersQoS : public QoS {
  public:
    constexpr ParametersQoS() : QoS(::rclcpp::detail::qos_table::parameters()) {}
};

/// `rmw_qos_profile_parameter_events` — KEEP_LAST(1000), RELIABLE, VOLATILE.
/// ADOPT. A ported node that publishes parameter events names it.
///
/// This is the profile phase-428 F2 found inverted on the RUST side
/// (`nros-rmw/src/traits.rs`, `KeepAll, 0`). The C++ value was already right;
/// two surfaces disagreeing about one upstream constant, with the wrong one
/// carrying a comment claiming it matched, is what W10's single table is for.
class ParameterEventsQoS : public QoS {
  public:
    constexpr ParameterEventsQoS() : QoS(::rclcpp::detail::qos_table::parameter_events()) {}
};

/// `rcl_qos_profile_rosout_default` — KEEP_LAST(1000), RELIABLE,
/// TRANSIENT_LOCAL, lifespan 10 s. ADOPT.
///
/// ADOPT-BOUNDED on one point, and it is about the TOPIC, not the profile: the
/// profile's four policies are transcribed exactly, but nano-ros publishes no
/// `/rosout` topic (logging goes to `nros_log`'s per-platform sink), so this
/// names a profile you can apply to a topic of your own rather than one the
/// runtime is already using.
class RosoutQoS : public QoS {
  public:
    constexpr RosoutQoS() : QoS(::rclcpp::detail::qos_table::rosout()) {}
};

/// `rclcpp::ClockQoS` — KEEP_LAST(1), BEST_EFFORT, VOLATILE. ADOPT.
class ClockQoS : public QoS {
  public:
    constexpr ClockQoS() : QoS(::rclcpp::detail::qos_table::clock()) {}
};

/// `rclcpp::BestAvailableQoS` — `rmw_qos_profile_best_available`. ADOPT
/// (issue 1973).
///
/// Its policies resolve when the entity is created, against the endpoints the
/// backend has already discovered on the topic — upstream's rule, applied once
/// for every backend by the runtime (`nros_rmw::QoSProfile::resolve_best_available`).
/// A backend with no graph (XRCE, uORB) has discovered nothing, which resolves
/// to upstream's own nothing-discovered answer. Read the outcome back with
/// `get_actual_qos()`. Upstream's caveat holds verbatim: the choice is not
/// revisited when endpoints appear later.
class BestAvailableQoS : public QoS {
  public:
    constexpr BestAvailableQoS() : QoS(::rclcpp::detail::qos_table::best_available()) {}
};

/// `rclcpp::SystemDefaultsQoS` — **REFUSE-LOUD**.
///
/// Every field of `rmw_qos_profile_system_default` is a sentinel meaning "let
/// the RMW decide", and issue 0829 measured the two reference RMWs resolving
/// the depth sentinel to different numbers (Cyclone 1, zenoh 42). `rclcpp::QoS`
/// has no sentinel, deliberately — the backend is linked at build time, so
/// there is nothing to defer to. Any concrete value here would be a different
/// profile wearing this name, which is precisely what the old `QoS(10)` was.
class SystemDefaultsQoS : public QoS {
  public:
    template <typename T = void> SystemDefaultsQoS() {
        static_assert(detail::refuse<T>::value, NROS_RCLCPP_REFUSE_SYSTEM_DEFAULTS_QOS);
    }
};

/// `rclcpp::KeepLast(depth)`.
///
/// ADOPT-BOUNDED: upstream returns a `QoSInitialization` — a history/depth pair
/// a `QoS` is then built from — and this returns a whole `QoS` carrying the
/// default reliability and durability. The porting spellings that matter,
/// `rclcpp::QoS(rclcpp::KeepLast(10))` and passing it straight to
/// `create_publisher`, both resolve to the same profile either way. What does
/// NOT carry over is using it as an initialiser for a profile whose other
/// policies you meant to keep — `rclcpp::SensorDataQoS(rclcpp::KeepLast(1))`
/// has no equivalent here; write `rclcpp::QoS().best_effort().keep_last(1)`.
constexpr QoS KeepLast(::size_t depth) {
    return QoS(depth);
}

/// `rclcpp::KeepAll()`. Same ADOPT-BOUNDED note as `KeepLast`.
constexpr QoS KeepAll() {
    return QoS(::rclcpp::QoS().keep_all());
}

} // namespace rclcpp

// phase-488, issue 2054 — upstream's `rmw_qos_profile_t`, its policy enums and
// its `rmw_qos_profile_*` constants, as ported code spells them:
//
//     create_service<S>(name, cb, rmw_qos_profile_services_default, group);
//     rmw_qos_profile_t profile = rmw_qos_profile_default;
//     profile.depth = 1;
//     rclcpp::QoS qos(rclcpp::QoSInitialization::from_rmw(profile), profile);
//
// Field names, field types and enumerator values are upstream's
// (`rmw/types.h` and `rmw/time.h`, ROS 2 Humble; the BEST_AVAILABLE values are
// Iron's, and Humble code never names them). A profile converts IMPLICITLY to
// `rclcpp::QoS`, which is what every verb here takes, so Humble's
// `create_service(..., const rmw_qos_profile_t&, ...)` call sites compile
// unchanged; `QoS::get_rmw_qos_profile()` goes back.
//
// On every target, as the rest of this header is: RFC-0096 D1 is one API on
// every platform, and `rclcpp::QoSInitialization` is a public type. Nothing
// here needs the STL.
//
// The RMW ABI's `<nros/rmw_entity.h>` also defines a struct named
// `rmw_qos_profile_t` — the backend vtable's, with ABI field widths
// (`uint16_t depth`, `uint32_t deadline_ms`). That header is for a backend's
// implementation, never for a node, so a translation unit that includes both
// is a backend that also includes rclcpp; it is refused below with the reason
// rather than left to a redefinition error naming two files.
//
// `rmw_qos_profile_system_default` is still absent: every field of it is a
// sentinel, which a `QoS` request may not carry (see `qos_table`), and the two
// reference RMWs resolve it differently (issue 0829).
#if defined(NROS_RMW_ENTITY_H)
#error                                                                                             \
    "<rclcpp/qos.hpp> defines upstream's rmw_qos_profile_t, and <nros/rmw_entity.h> defines the RMW ABI's struct under the same name. A node includes rclcpp only; a backend includes the ABI header only (issue 2054)."
#endif

#include <stddef.h>

/// Upstream's `rmw_time_t` (`rmw/time.h`): a duration as seconds plus
/// nanoseconds, both unsigned.
typedef struct rmw_time_s {
    uint64_t sec;
    uint64_t nsec;
} rmw_time_t;

#define RMW_DURATION_INFINITE                                                                      \
    { 9223372036LL, 854775807LL }
#define RMW_DURATION_UNSPECIFIED                                                                   \
    { 0LL, 0LL }
#define RMW_QOS_POLICY_DEPTH_SYSTEM_DEFAULT 0
#define RMW_QOS_DEADLINE_DEFAULT RMW_DURATION_UNSPECIFIED
#define RMW_QOS_DEADLINE_BEST_AVAILABLE                                                            \
    { 9223372036LL, 854775806LL }
#define RMW_QOS_LIFESPAN_DEFAULT RMW_DURATION_UNSPECIFIED
#define RMW_QOS_LIVELINESS_LEASE_DURATION_DEFAULT RMW_DURATION_UNSPECIFIED
#define RMW_QOS_LIVELINESS_LEASE_DURATION_BEST_AVAILABLE                                           \
    { 9223372036LL, 854775806LL }

typedef enum rmw_qos_reliability_policy_e {
    RMW_QOS_POLICY_RELIABILITY_SYSTEM_DEFAULT = 0,
    RMW_QOS_POLICY_RELIABILITY_RELIABLE = 1,
    RMW_QOS_POLICY_RELIABILITY_BEST_EFFORT = 2,
    RMW_QOS_POLICY_RELIABILITY_UNKNOWN = 3,
    RMW_QOS_POLICY_RELIABILITY_BEST_AVAILABLE = 4,
} rmw_qos_reliability_policy_t;

typedef enum rmw_qos_history_policy_e {
    RMW_QOS_POLICY_HISTORY_SYSTEM_DEFAULT = 0,
    RMW_QOS_POLICY_HISTORY_KEEP_LAST = 1,
    RMW_QOS_POLICY_HISTORY_KEEP_ALL = 2,
    RMW_QOS_POLICY_HISTORY_UNKNOWN = 3,
} rmw_qos_history_policy_t;

typedef enum rmw_qos_durability_policy_e {
    RMW_QOS_POLICY_DURABILITY_SYSTEM_DEFAULT = 0,
    RMW_QOS_POLICY_DURABILITY_TRANSIENT_LOCAL = 1,
    RMW_QOS_POLICY_DURABILITY_VOLATILE = 2,
    RMW_QOS_POLICY_DURABILITY_UNKNOWN = 3,
    RMW_QOS_POLICY_DURABILITY_BEST_AVAILABLE = 4,
} rmw_qos_durability_policy_t;

typedef enum rmw_qos_liveliness_policy_e {
    RMW_QOS_POLICY_LIVELINESS_SYSTEM_DEFAULT = 0,
    RMW_QOS_POLICY_LIVELINESS_AUTOMATIC = 1,
    /// Deprecated upstream; kept because the value is upstream's.
    RMW_QOS_POLICY_LIVELINESS_MANUAL_BY_NODE = 2,
    RMW_QOS_POLICY_LIVELINESS_MANUAL_BY_TOPIC = 3,
    RMW_QOS_POLICY_LIVELINESS_UNKNOWN = 4,
    RMW_QOS_POLICY_LIVELINESS_BEST_AVAILABLE = 5,
} rmw_qos_liveliness_policy_t;

namespace rclcpp {
namespace detail {

/// A C ABI window (milliseconds, `0` = unset) as upstream's `rmw_time_t`.
///
/// `0` is upstream's UNSPECIFIED `{0, 0}`, which is what every upstream profile
/// states and what both sides read as "no window". The two sentinels map onto
/// upstream's: `UINT32_MAX` (explicit infinite) onto `RMW_DURATION_INFINITE`,
/// `UINT32_MAX - 1` (best available) onto `RMW_QOS_DEADLINE_BEST_AVAILABLE`.
constexpr rmw_time_t qos_window_to_rmw(uint32_t ms) {
    return ms == 0u                ? rmw_time_t{0u, 0u}
           : ms == UINT32_MAX      ? rmw_time_t{9223372036u, 854775807u}
           : ms == UINT32_MAX - 1u ? rmw_time_t{9223372036u, 854775806u}
                                   : rmw_time_t{ms / 1000u, (ms % 1000u) * 1000000u};
}

/// The inverse, rounding as `qos_window_ms` does: up to the next whole
/// millisecond, saturating to infinite, never wrapping into a short window.
constexpr uint32_t qos_window_from_rmw(const rmw_time_t& t) {
    return (t.sec == 0u && t.nsec == 0u)                    ? 0u
           : (t.sec == 9223372036u && t.nsec == 854775807u) ? UINT32_MAX
           : (t.sec == 9223372036u && t.nsec == 854775806u)
               ? UINT32_MAX - 1u
               : (t.sec > 9000000000u ? UINT32_MAX
                                      : qos_window_ms(Duration::from_nanoseconds(
                                            static_cast<int64_t>(t.sec) * 1000000000 +
                                            static_cast<int64_t>(t.nsec))));
}

// Each policy as a switch, because the two vocabularies number them
// differently (`rclcpp::ReliabilityPolicy::Reliable` is 0, upstream's
// `RMW_QOS_POLICY_RELIABILITY_RELIABLE` is 1), so no cast could be right.
constexpr rmw_qos_reliability_policy_t qos_reliability_to_rmw(ReliabilityPolicy p) {
    return p == ReliabilityPolicy::Reliable        ? RMW_QOS_POLICY_RELIABILITY_RELIABLE
           : p == ReliabilityPolicy::BestEffort    ? RMW_QOS_POLICY_RELIABILITY_BEST_EFFORT
           : p == ReliabilityPolicy::Unknown       ? RMW_QOS_POLICY_RELIABILITY_UNKNOWN
           : p == ReliabilityPolicy::BestAvailable ? RMW_QOS_POLICY_RELIABILITY_BEST_AVAILABLE
                                                   : RMW_QOS_POLICY_RELIABILITY_SYSTEM_DEFAULT;
}
constexpr ReliabilityPolicy qos_reliability_from_rmw(rmw_qos_reliability_policy_t p) {
    return p == RMW_QOS_POLICY_RELIABILITY_RELIABLE         ? ReliabilityPolicy::Reliable
           : p == RMW_QOS_POLICY_RELIABILITY_BEST_EFFORT    ? ReliabilityPolicy::BestEffort
           : p == RMW_QOS_POLICY_RELIABILITY_UNKNOWN        ? ReliabilityPolicy::Unknown
           : p == RMW_QOS_POLICY_RELIABILITY_BEST_AVAILABLE ? ReliabilityPolicy::BestAvailable
                                                            : ReliabilityPolicy::SystemDefault;
}
constexpr rmw_qos_history_policy_t qos_history_to_rmw(HistoryPolicy p) {
    return p == HistoryPolicy::KeepLast  ? RMW_QOS_POLICY_HISTORY_KEEP_LAST
           : p == HistoryPolicy::KeepAll ? RMW_QOS_POLICY_HISTORY_KEEP_ALL
           : p == HistoryPolicy::Unknown ? RMW_QOS_POLICY_HISTORY_UNKNOWN
                                         : RMW_QOS_POLICY_HISTORY_SYSTEM_DEFAULT;
}
constexpr HistoryPolicy qos_history_from_rmw(rmw_qos_history_policy_t p) {
    return p == RMW_QOS_POLICY_HISTORY_KEEP_LAST  ? HistoryPolicy::KeepLast
           : p == RMW_QOS_POLICY_HISTORY_KEEP_ALL ? HistoryPolicy::KeepAll
           : p == RMW_QOS_POLICY_HISTORY_UNKNOWN  ? HistoryPolicy::Unknown
                                                  : HistoryPolicy::SystemDefault;
}
constexpr rmw_qos_durability_policy_t qos_durability_to_rmw(DurabilityPolicy p) {
    return p == DurabilityPolicy::Volatile         ? RMW_QOS_POLICY_DURABILITY_VOLATILE
           : p == DurabilityPolicy::TransientLocal ? RMW_QOS_POLICY_DURABILITY_TRANSIENT_LOCAL
           : p == DurabilityPolicy::Unknown        ? RMW_QOS_POLICY_DURABILITY_UNKNOWN
           : p == DurabilityPolicy::BestAvailable  ? RMW_QOS_POLICY_DURABILITY_BEST_AVAILABLE
                                                   : RMW_QOS_POLICY_DURABILITY_SYSTEM_DEFAULT;
}
constexpr DurabilityPolicy qos_durability_from_rmw(rmw_qos_durability_policy_t p) {
    return p == RMW_QOS_POLICY_DURABILITY_VOLATILE          ? DurabilityPolicy::Volatile
           : p == RMW_QOS_POLICY_DURABILITY_TRANSIENT_LOCAL ? DurabilityPolicy::TransientLocal
           : p == RMW_QOS_POLICY_DURABILITY_UNKNOWN         ? DurabilityPolicy::Unknown
           : p == RMW_QOS_POLICY_DURABILITY_BEST_AVAILABLE  ? DurabilityPolicy::BestAvailable
                                                            : DurabilityPolicy::SystemDefault;
}
constexpr rmw_qos_liveliness_policy_t qos_liveliness_to_rmw(LivelinessPolicy p) {
    return p == LivelinessPolicy::Automatic       ? RMW_QOS_POLICY_LIVELINESS_AUTOMATIC
           : p == LivelinessPolicy::ManualByTopic ? RMW_QOS_POLICY_LIVELINESS_MANUAL_BY_TOPIC
           : p == LivelinessPolicy::ManualByNode  ? RMW_QOS_POLICY_LIVELINESS_MANUAL_BY_NODE
           : p == LivelinessPolicy::Unknown       ? RMW_QOS_POLICY_LIVELINESS_UNKNOWN
           : p == LivelinessPolicy::BestAvailable ? RMW_QOS_POLICY_LIVELINESS_BEST_AVAILABLE
                                                  : RMW_QOS_POLICY_LIVELINESS_SYSTEM_DEFAULT;
}
constexpr LivelinessPolicy qos_liveliness_from_rmw(rmw_qos_liveliness_policy_t p) {
    return p == RMW_QOS_POLICY_LIVELINESS_AUTOMATIC         ? LivelinessPolicy::Automatic
           : p == RMW_QOS_POLICY_LIVELINESS_MANUAL_BY_TOPIC ? LivelinessPolicy::ManualByTopic
           : p == RMW_QOS_POLICY_LIVELINESS_MANUAL_BY_NODE  ? LivelinessPolicy::ManualByNode
           : p == RMW_QOS_POLICY_LIVELINESS_UNKNOWN         ? LivelinessPolicy::Unknown
           : p == RMW_QOS_POLICY_LIVELINESS_BEST_AVAILABLE  ? LivelinessPolicy::BestAvailable
                                                            : LivelinessPolicy::SystemDefault;
}

} // namespace detail
} // namespace rclcpp

/// Upstream's `rmw_qos_profile_t` (`rmw/types.h`): same fields, same order,
/// same types. An aggregate, so upstream's brace initialisers compile.
///
/// The one addition is the conversion to `rclcpp::QoS`, which is what lets a
/// profile reach a verb that takes `QoS` — Humble's `create_service` takes the
/// struct, ours takes `QoS`. A profile carrying a `SYSTEM_DEFAULT` or `UNKNOWN`
/// policy converts to a `QoS` carrying the same sentinel, exactly as a
/// `get_actual_qos()` read-back does; no constant below carries one except
/// liveliness, which every upstream profile leaves to the middleware.
typedef struct rmw_qos_profile_s {
    rmw_qos_history_policy_t history;
    size_t depth;
    rmw_qos_reliability_policy_t reliability;
    rmw_qos_durability_policy_t durability;
    rmw_time_t deadline;
    rmw_time_t lifespan;
    rmw_qos_liveliness_policy_t liveliness;
    rmw_time_t liveliness_lease_duration;
    bool avoid_ros_namespace_conventions;

    // NOLINTNEXTLINE(google-explicit-constructor)
    constexpr operator ::rclcpp::QoS() const {
        nros_cpp_qos_t f{};
        f.reliability = static_cast<nros_cpp_qos_reliability_t>(
            ::rclcpp::detail::qos_reliability_from_rmw(reliability));
        f.durability = static_cast<nros_cpp_qos_durability_t>(
            ::rclcpp::detail::qos_durability_from_rmw(durability));
        f.history =
            static_cast<nros_cpp_qos_history_t>(::rclcpp::detail::qos_history_from_rmw(history));
        f.liveliness_kind = static_cast<nros_cpp_qos_liveliness_t>(
            ::rclcpp::detail::qos_liveliness_from_rmw(liveliness));
        f.depth = static_cast<int>(depth);
        f.deadline_ms = ::rclcpp::detail::qos_window_from_rmw(deadline);
        f.lifespan_ms = ::rclcpp::detail::qos_window_from_rmw(lifespan);
        f.liveliness_lease_ms = ::rclcpp::detail::qos_window_from_rmw(liveliness_lease_duration);
        f.avoid_ros_namespace_conventions = avoid_ros_namespace_conventions ? 1 : 0;
        return ::rclcpp::detail::qos_from_ffi(f);
    }
} rmw_qos_profile_t;

namespace rclcpp {
namespace detail {

/// `QoS` → upstream's struct, field by field. `tx_express` has no upstream
/// field and is dropped, as it is from every upstream-shaped read.
constexpr rmw_qos_profile_t qos_to_rmw(const QoS& q) {
    return rmw_qos_profile_t{qos_history_to_rmw(q.history()),
                             static_cast<size_t>(q.depth() < 0 ? 0 : q.depth()),
                             qos_reliability_to_rmw(q.reliability()),
                             qos_durability_to_rmw(q.durability()),
                             qos_window_to_rmw(qos_to_ffi(q).deadline_ms),
                             qos_window_to_rmw(qos_to_ffi(q).lifespan_ms),
                             qos_liveliness_to_rmw(q.liveliness()),
                             qos_window_to_rmw(qos_to_ffi(q).liveliness_lease_ms),
                             q.avoid_ros_namespace_conventions()};
}

} // namespace detail

/// Upstream's `QoSInitialization` (`rclcpp/qos.hpp`): the history policy and
/// depth a `QoS` is built from. ADOPT-BOUNDED: upstream's `KeepLast` and
/// `KeepAll` derive from it, and ours are functions returning a whole `QoS`
/// (see `KeepLast` above), so this converts implicitly from a `QoS` instead,
/// taking its history and depth — `rclcpp::QoS(rclcpp::KeepLast(1), profile)`
/// compiles either way.
struct QoSInitialization {
    rmw_qos_history_policy_t history_policy;
    size_t depth;

    constexpr QoSInitialization(rmw_qos_history_policy_t history_policy_arg, size_t depth_arg)
        : history_policy(history_policy_arg), depth(depth_arg) {}

    // NOLINTNEXTLINE(google-explicit-constructor)
    constexpr QoSInitialization(const QoS& qos)
        : history_policy(detail::qos_history_to_rmw(qos.history())),
          depth(static_cast<size_t>(qos.depth() < 0 ? 0 : qos.depth())) {}

    static constexpr QoSInitialization from_rmw(const rmw_qos_profile_t& profile) {
        return QoSInitialization(profile.history, profile.depth);
    }
};

constexpr QoS::QoS(const QoSInitialization& init, const ::rmw_qos_profile_s& profile)
    : QoS(static_cast<QoS>(profile)) {
    history_ = detail::qos_history_from_rmw(init.history_policy);
    depth_ = static_cast<int>(init.depth);
}

constexpr ::rmw_qos_profile_s QoS::get_rmw_qos_profile() const {
    return detail::qos_to_rmw(*this);
}

} // namespace rclcpp

/// Upstream's five concrete profiles, each the row of `rclcpp::detail::
/// qos_table` lowered to upstream's struct, so the table is still the one place
/// a profile's policies are spelled.
static constexpr rmw_qos_profile_t rmw_qos_profile_default =
    ::rclcpp::detail::qos_to_rmw(::rclcpp::detail::qos_table::default_profile());
static constexpr rmw_qos_profile_t rmw_qos_profile_sensor_data =
    ::rclcpp::detail::qos_to_rmw(::rclcpp::detail::qos_table::sensor_data());
static constexpr rmw_qos_profile_t rmw_qos_profile_services_default =
    ::rclcpp::detail::qos_to_rmw(::rclcpp::detail::qos_table::services_default());
static constexpr rmw_qos_profile_t rmw_qos_profile_parameters =
    ::rclcpp::detail::qos_to_rmw(::rclcpp::detail::qos_table::parameters());
static constexpr rmw_qos_profile_t rmw_qos_profile_parameter_events =
    ::rclcpp::detail::qos_to_rmw(::rclcpp::detail::qos_table::parameter_events());

namespace rclcpp {
// Defined after the constants because it starts from one of them.
constexpr QoS::QoS(const QoSInitialization& init) : QoS(init, ::rmw_qos_profile_default) {}
} // namespace rclcpp

#endif // NROS_RCLCPP_QOS_HPP
