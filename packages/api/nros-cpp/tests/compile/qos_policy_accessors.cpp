// Phase 379 W5 — the C++ QoS surface under its rclcpp names, and the old
// spellings, which phase-482 W6 deleted.
//
// Three renames landed together (ledger: `cpp:ReliabilityPolicy` and its three
// siblings, `cpp:QoS::reliability_raw` and its three, `cpp:QoS::deadline` and
// its two):
//
//   D3  the four policy enums are PUBLIC and at NAMESPACE scope, under
//       rclcpp's names — `rclcpp::ReliabilityPolicy` etc.
//   D4  the getters return those enums instead of `int`, so `reliability()`
//       replaces `reliability_raw()`.
//   D5  the three time windows take and return `rclcpp::Duration`, so
//       `deadline()` / `lifespan()` / `liveliness_lease_duration()` replace
//       the `_ms`-suffixed pair.
//
// This TU asserts the RETURN TYPES, not just that the names resolve: a getter
// that came back as `int` under the new name would satisfy a name check and
// re-introduce exactly the defect the ledger recorded. Every assertion is
// `static_assert` over `constexpr` calls, so the profile is also proven to be
// computable at compile time — which is why the family is `constexpr` at all.
//
// The `-Werror=deprecated-declarations` half is `qos_deprecation_probe.cpp`.
// This file is compiled with `-Wno-deprecated-declarations`, because it names
// the deprecated spellings on purpose.

#include "nros/qos.hpp"

#include <type_traits>

namespace {

using rclcpp::Duration;
using rclcpp::QoS;

// -- D3: the policy enums are namespace-scope, public, and rclcpp-named ----

static_assert(std::is_enum<rclcpp::ReliabilityPolicy>::value, "ReliabilityPolicy must be an enum");
static_assert(std::is_enum<rclcpp::DurabilityPolicy>::value, "DurabilityPolicy must be an enum");
static_assert(std::is_enum<rclcpp::HistoryPolicy>::value, "HistoryPolicy must be an enum");
static_assert(std::is_enum<rclcpp::LivelinessPolicy>::value, "LivelinessPolicy must be an enum");

// The wire values are the C ABI's, and they are what `detail::qos_to_ffi`
// static_casts across. A reordering here is an ABI break, not a rename.
static_assert(static_cast<int>(rclcpp::ReliabilityPolicy::Reliable) == 0 &&
                  static_cast<int>(rclcpp::ReliabilityPolicy::BestEffort) == 1,
              "reliability values are ABI");
static_assert(static_cast<int>(rclcpp::DurabilityPolicy::Volatile) == 0 &&
                  static_cast<int>(rclcpp::DurabilityPolicy::TransientLocal) == 1,
              "durability values are ABI");
static_assert(static_cast<int>(rclcpp::HistoryPolicy::KeepLast) == 0 &&
                  static_cast<int>(rclcpp::HistoryPolicy::KeepAll) == 1,
              "history values are ABI");
static_assert(static_cast<int>(rclcpp::LivelinessPolicy::SystemDefault) == 0 &&
                  static_cast<int>(rclcpp::LivelinessPolicy::Automatic) == 1 &&
                  static_cast<int>(rclcpp::LivelinessPolicy::ManualByTopic) == 2 &&
                  static_cast<int>(rclcpp::LivelinessPolicy::ManualByNode) == 3,
              "liveliness values are ABI");

// SCOPED, as rclcpp's are (phase-483 W1). In `rclcpp::` an unscoped
// `KeepLast` enumerator would collide with upstream's `rclcpp::KeepLast`
// profile helper, and the prefixed `LivelinessManualByTopic` spellings existed
// only to keep unscoped enumerators apart.
static_assert(!std::is_convertible<rclcpp::ReliabilityPolicy, int>::value,
              "the policy enums are enum class, as upstream's");
static_assert(!std::is_convertible<rclcpp::LivelinessPolicy, int>::value,
              "the policy enums are enum class, as upstream's");

// -- D4: the getters return the policy, not `int` -------------------------

static_assert(std::is_same<decltype(std::declval<const QoS&>().reliability()),
                           rclcpp::ReliabilityPolicy>::value,
              "QoS::reliability() must return ReliabilityPolicy");
static_assert(std::is_same<decltype(std::declval<const QoS&>().durability()),
                           rclcpp::DurabilityPolicy>::value,
              "QoS::durability() must return DurabilityPolicy");
static_assert(
    std::is_same<decltype(std::declval<const QoS&>().history()), rclcpp::HistoryPolicy>::value,
    "QoS::history() must return HistoryPolicy");
static_assert(std::is_same<decltype(std::declval<const QoS&>().liveliness()),
                           rclcpp::LivelinessPolicy>::value,
              "QoS::liveliness() must return LivelinessPolicy");

// `liveliness` is an OVERLOAD PAIR, as it is in rclcpp: the 0-arg getter and
// the 1-arg setter. Losing either is the shape defect the ledger recorded.
static_assert(
    std::is_same<decltype(std::declval<QoS&>().liveliness(rclcpp::LivelinessPolicy::Automatic)),
                 QoS&>::value,
    "QoS::liveliness(LivelinessPolicy) must stay a chainable setter");

static_assert(QoS().reliability() == rclcpp::ReliabilityPolicy::Reliable,
              "default profile is reliable");
static_assert(QoS().best_effort().reliability() == rclcpp::ReliabilityPolicy::BestEffort,
              "best_effort() sets it");
static_assert(QoS().transient_local().durability() == rclcpp::DurabilityPolicy::TransientLocal,
              "transient_local()");
static_assert(QoS().keep_all().history() == rclcpp::HistoryPolicy::KeepAll, "keep_all()");
static_assert(QoS().liveliness(rclcpp::LivelinessPolicy::ManualByNode).liveliness() ==
                  rclcpp::LivelinessPolicy::ManualByNode,
              "liveliness() round-trips");

// -- D5: the three windows are `Duration` --------------------------------

static_assert(std::is_same<decltype(std::declval<const QoS&>().deadline()), Duration>::value,
              "QoS::deadline() must return rclcpp::Duration");
static_assert(std::is_same<decltype(std::declval<const QoS&>().lifespan()), Duration>::value,
              "QoS::lifespan() must return rclcpp::Duration");
static_assert(
    std::is_same<decltype(std::declval<const QoS&>().liveliness_lease_duration()), Duration>::value,
    "QoS::liveliness_lease_duration() must return rclcpp::Duration");

static_assert(QoS().deadline().nanoseconds() == 0, "no deadline by default");
static_assert(QoS().deadline(Duration::from_seconds(0.1)).deadline() == Duration(0, 100000000u),
              "a whole-millisecond deadline round-trips exactly");
static_assert(QoS().lifespan(Duration(2, 0)).lifespan() == Duration(2, 0),
              "a whole-second lifespan round-trips exactly");
static_assert(QoS().liveliness_lease_duration(Duration(0, 5000000u)).liveliness_lease_duration() ==
                  Duration(0, 5000000u),
              "a whole-millisecond lease round-trips exactly");

// The boundary the doc comment promises. `0` means INFINITE in the C ABI, so a
// sub-millisecond window must NOT truncate into it — it rounds UP to 1 ms.
static_assert(rclcpp::detail::qos_window_ms(Duration::from_nanoseconds(1)) == 1u,
              "1 ns must round UP to 1 ms, never down to the infinite sentinel");
static_assert(rclcpp::detail::qos_window_ms(Duration::from_nanoseconds(999999)) == 1u,
              "999999 ns rounds up to 1 ms");
static_assert(rclcpp::detail::qos_window_ms(Duration::from_nanoseconds(1000000)) == 1u,
              "exactly 1 ms is 1 ms");
static_assert(rclcpp::detail::qos_window_ms(Duration::from_nanoseconds(1000001)) == 2u,
              "1 ms + 1 ns rounds up to 2 ms");
static_assert(rclcpp::detail::qos_window_ms(Duration()) == 0u, "zero stays the infinite sentinel");
static_assert(rclcpp::detail::qos_window_ms(Duration::from_nanoseconds(-5)) == 0u,
              "a negative window is the unset spelling, not a wrapped huge one");
static_assert(rclcpp::detail::qos_window_ms(Duration::max()) == UINT32_MAX,
              "an over-long window saturates rather than wrapping short");
static_assert(QoS().deadline(Duration::from_nanoseconds(1)).deadline() ==
                  Duration::from_nanoseconds(1000000),
              "the sub-millisecond deadline is readable back as the 1 ms it became");

// -- The C ABI record is unchanged ---------------------------------------
//
// The same token `deadline_ms` is a struct FIELD here and was a class METHOD
// until phase-482 W6 deleted it; the METHOD went and the FIELD must not. A textual sweep that
// renamed both would break the by-value ABI silently (issue 0160's class).

constexpr nros_cpp_qos_t kMarshalled =
    rclcpp::detail::qos_to_ffi(QoS()
                                   .deadline(Duration(0, 100000000u))
                                   .lifespan(Duration(1, 0))
                                   .liveliness_lease_duration(Duration(0, 5000000u))
                                   .best_effort()
                                   .transient_local()
                                   .keep_all()
                                   .liveliness(rclcpp::LivelinessPolicy::ManualByNode)
                                   .tx_express(true));

static_assert(std::is_same<decltype(kMarshalled.deadline_ms), uint32_t>::value,
              "nros_cpp_qos_t.deadline_ms is uint32_t milliseconds and must not move");
static_assert(std::is_same<decltype(kMarshalled.lifespan_ms), uint32_t>::value,
              "nros_cpp_qos_t.lifespan_ms is uint32_t milliseconds and must not move");
static_assert(std::is_same<decltype(kMarshalled.liveliness_lease_ms), uint32_t>::value,
              "nros_cpp_qos_t.liveliness_lease_ms is uint32_t milliseconds and must not move");

static_assert(kMarshalled.deadline_ms == 100u, "100 ms deadline reaches the ABI as 100");
static_assert(kMarshalled.lifespan_ms == 1000u, "a 1 s lifespan reaches the ABI as 1000");
static_assert(kMarshalled.liveliness_lease_ms == 5u, "a 5 ms lease reaches the ABI as 5");
static_assert(kMarshalled.reliability == NROS_CPP_QOS_BEST_EFFORT, "reliability marshals");
static_assert(kMarshalled.durability == NROS_CPP_QOS_TRANSIENT_LOCAL, "durability marshals");
static_assert(kMarshalled.history == NROS_CPP_QOS_KEEP_ALL, "history marshals");
static_assert(kMarshalled.liveliness_kind == NROS_CPP_QOS_LIVELINESS_MANUAL_BY_NODE,
              "liveliness marshals");
static_assert(kMarshalled.tx_express == 1, "tx_express marshals");

// -- issue 1437: the two sentinels, and the mirror they depend on ---------
//
// `detail::qos_to_ffi` and `detail::qos_from_ffi` `static_cast` between
// `rclcpp::ReliabilityPolicy` and `nros_cpp_qos_reliability_t` (and three
// siblings). That is only correct while the two vocabularies agree
// ENUMERATOR FOR ENUMERATOR, and the phase-444 sentinels made each of them
// longer — the exact shape in which a mirror goes stale. So the agreement is
// MEASURED here rather than asserted in a comment beside the cast.
//
// NOTE these are NOT the RMW ABI's numbers. `<nros/rmw_entity.h>` spells
// reliability SYSTEM_DEFAULT 0 / RELIABLE 1 / BEST_EFFORT 2, durability
// SYSTEM_DEFAULT 0 / TRANSIENT_LOCAL 1 / VOLATILE 2, history SYSTEM_DEFAULT 0
// / KEEP_LAST 1 / KEEP_ALL 2, and liveliness with MANUAL_BY_NODE and
// MANUAL_BY_TOPIC TRANSPOSED against this header. All four differ; nothing
// may cast between these values and those.

static_assert(static_cast<int>(rclcpp::ReliabilityPolicy::SystemDefault) ==
                      NROS_CPP_QOS_RELIABILITY_SYSTEM_DEFAULT &&
                  static_cast<int>(rclcpp::ReliabilityPolicy::Unknown) ==
                      NROS_CPP_QOS_RELIABILITY_UNKNOWN &&
                  static_cast<int>(rclcpp::ReliabilityPolicy::Reliable) == NROS_CPP_QOS_RELIABLE &&
                  static_cast<int>(rclcpp::ReliabilityPolicy::BestEffort) ==
                      NROS_CPP_QOS_BEST_EFFORT,
              "rclcpp::ReliabilityPolicy must mirror nros_cpp_qos_reliability_t value for value");
static_assert(static_cast<int>(rclcpp::DurabilityPolicy::SystemDefault) ==
                      NROS_CPP_QOS_DURABILITY_SYSTEM_DEFAULT &&
                  static_cast<int>(rclcpp::DurabilityPolicy::Unknown) ==
                      NROS_CPP_QOS_DURABILITY_UNKNOWN &&
                  static_cast<int>(rclcpp::DurabilityPolicy::Volatile) == NROS_CPP_QOS_VOLATILE &&
                  static_cast<int>(rclcpp::DurabilityPolicy::TransientLocal) ==
                      NROS_CPP_QOS_TRANSIENT_LOCAL,
              "rclcpp::DurabilityPolicy must mirror nros_cpp_qos_durability_t value for value");
static_assert(static_cast<int>(rclcpp::HistoryPolicy::SystemDefault) ==
                      NROS_CPP_QOS_HISTORY_SYSTEM_DEFAULT &&
                  static_cast<int>(rclcpp::HistoryPolicy::Unknown) ==
                      NROS_CPP_QOS_HISTORY_UNKNOWN &&
                  static_cast<int>(rclcpp::HistoryPolicy::KeepLast) == NROS_CPP_QOS_KEEP_LAST &&
                  static_cast<int>(rclcpp::HistoryPolicy::KeepAll) == NROS_CPP_QOS_KEEP_ALL,
              "rclcpp::HistoryPolicy must mirror nros_cpp_qos_history_t value for value");
static_assert(static_cast<int>(rclcpp::LivelinessPolicy::Unknown) ==
                      NROS_CPP_QOS_LIVELINESS_UNKNOWN &&
                  static_cast<int>(rclcpp::LivelinessPolicy::SystemDefault) ==
                      NROS_CPP_QOS_LIVELINESS_NONE &&
                  static_cast<int>(rclcpp::LivelinessPolicy::Automatic) ==
                      NROS_CPP_QOS_LIVELINESS_AUTOMATIC &&
                  static_cast<int>(rclcpp::LivelinessPolicy::ManualByTopic) ==
                      NROS_CPP_QOS_LIVELINESS_MANUAL_BY_TOPIC &&
                  static_cast<int>(rclcpp::LivelinessPolicy::ManualByNode) ==
                      NROS_CPP_QOS_LIVELINESS_MANUAL_BY_NODE,
              "rclcpp::LivelinessPolicy must mirror nros_cpp_qos_liveliness_t value for value");

// The sentinels are APPENDED. Renumbering one would be an ABI break for every
// shipped image, and it would be invisible: the enumerator names would still
// resolve.
static_assert(static_cast<int>(rclcpp::ReliabilityPolicy::SystemDefault) == 2 &&
                  static_cast<int>(rclcpp::ReliabilityPolicy::Unknown) == 3,
              "the reliability sentinels are appended at 2 and 3");
static_assert(static_cast<int>(rclcpp::DurabilityPolicy::SystemDefault) == 2 &&
                  static_cast<int>(rclcpp::DurabilityPolicy::Unknown) == 3,
              "the durability sentinels are appended at 2 and 3");
static_assert(static_cast<int>(rclcpp::HistoryPolicy::SystemDefault) == 2 &&
                  static_cast<int>(rclcpp::HistoryPolicy::Unknown) == 3,
              "the history sentinels are appended at 2 and 3");
static_assert(static_cast<int>(rclcpp::LivelinessPolicy::Unknown) == 4,
              "the liveliness sentinel is appended at 4");

// `qos_from_ffi` is `qos_to_ffi`'s inverse, including for a profile NO SETTER
// CAN BUILD — which is the whole reason it writes `QoS`'s members directly.
// `qos_all_unknown()` is that profile and is itself built through
// `qos_from_ffi`, so asserting on it measures both.
//
// Not written as a `constexpr` lambda initialising a `nros_cpp_qos_t`: this TU
// is compiled at C++14 as well (the freestanding syntax probe), where a lambda
// is not implicitly `constexpr` and the whole block goes non-constant.

static_assert(rclcpp::detail::qos_all_unknown().reliability() == rclcpp::ReliabilityPolicy::Unknown,
              "an unreportable reliability survives the read-back conversion");
static_assert(rclcpp::detail::qos_all_unknown().liveliness() == rclcpp::LivelinessPolicy::Unknown,
              "an unreportable liveliness survives the read-back conversion");
static_assert(rclcpp::detail::qos_all_unknown().durability() == rclcpp::DurabilityPolicy::Unknown,
              "the all-absent profile is absent in every field");
static_assert(rclcpp::detail::qos_all_unknown().history() == rclcpp::HistoryPolicy::Unknown,
              "the all-absent profile is absent in every field");

// Round-trip through both conversions for an ordinary REQUEST profile: what a
// caller wrote is what a read-back of the same values reports.
static_assert(rclcpp::detail::qos_from_ffi(kMarshalled) ==
                  QoS()
                      .deadline(Duration(0, 100000000u))
                      .lifespan(Duration(1, 0))
                      .liveliness_lease_duration(Duration(0, 5000000u))
                      .best_effort()
                      .transient_local()
                      .keep_all()
                      .liveliness(rclcpp::LivelinessPolicy::ManualByNode)
                      .tx_express(true),
              "qos_from_ffi inverts qos_to_ffi");

} // namespace
