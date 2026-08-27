// Phase 379 W5 — the C++ QoS surface under its rclcpp names, and the old
// spellings kept alive as `[[deprecated]]` forwarders.
//
// Three renames landed together (ledger: `cpp:ReliabilityPolicy` and its three
// siblings, `cpp:QoS::reliability_raw` and its three, `cpp:QoS::deadline` and
// its two):
//
//   D3  the four policy enums are PUBLIC and at NAMESPACE scope, under
//       rclcpp's names — `nros::ReliabilityPolicy` etc.
//   D4  the getters return those enums instead of `int`, so `reliability()`
//       replaces `reliability_raw()`.
//   D5  the three time windows take and return `nros::Duration`, so
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

using nros::Duration;
using nros::QoS;

// -- D3: the policy enums are namespace-scope, public, and rclcpp-named ----

static_assert(std::is_enum<nros::ReliabilityPolicy>::value, "ReliabilityPolicy must be an enum");
static_assert(std::is_enum<nros::DurabilityPolicy>::value, "DurabilityPolicy must be an enum");
static_assert(std::is_enum<nros::HistoryPolicy>::value, "HistoryPolicy must be an enum");
static_assert(std::is_enum<nros::LivelinessPolicy>::value, "LivelinessPolicy must be an enum");

// The wire values are the C ABI's, and they are what `detail::qos_to_ffi`
// static_casts across. A reordering here is an ABI break, not a rename.
static_assert(nros::Reliable == 0 && nros::BestEffort == 1, "reliability values are ABI");
static_assert(nros::Volatile == 0 && nros::TransientLocal == 1, "durability values are ABI");
static_assert(nros::KeepLast == 0 && nros::KeepAll == 1, "history values are ABI");
static_assert(nros::LivelinessNone == 0 && nros::LivelinessAutomatic == 1 &&
                  nros::LivelinessManualByTopic == 2 && nros::LivelinessManualByNode == 3,
              "liveliness values are ABI");

// Unscoped, so BOTH spellings work: ours (historical, unqualified) and
// rclcpp's (enum-name qualified). Switching to `enum class` would break the
// first, and no ledger row makes that decision — see the note in qos.hpp.
static_assert(nros::ReliabilityPolicy::Reliable == nros::Reliable,
              "the enum-qualified rclcpp spelling must resolve");
static_assert(nros::LivelinessPolicy::LivelinessManualByTopic == nros::LivelinessManualByTopic,
              "the enum-qualified rclcpp spelling must resolve");

// -- D4: the getters return the policy, not `int` -------------------------

static_assert(std::is_same<decltype(std::declval<const QoS&>().reliability()),
                           nros::ReliabilityPolicy>::value,
              "QoS::reliability() must return ReliabilityPolicy");
static_assert(
    std::is_same<decltype(std::declval<const QoS&>().durability()), nros::DurabilityPolicy>::value,
    "QoS::durability() must return DurabilityPolicy");
static_assert(
    std::is_same<decltype(std::declval<const QoS&>().history()), nros::HistoryPolicy>::value,
    "QoS::history() must return HistoryPolicy");
static_assert(
    std::is_same<decltype(std::declval<const QoS&>().liveliness()), nros::LivelinessPolicy>::value,
    "QoS::liveliness() must return LivelinessPolicy");

// `liveliness` is an OVERLOAD PAIR, as it is in rclcpp: the 0-arg getter and
// the 1-arg setter. Losing either is the shape defect the ledger recorded.
static_assert(
    std::is_same<decltype(std::declval<QoS&>().liveliness(nros::LivelinessAutomatic)), QoS&>::value,
    "QoS::liveliness(LivelinessPolicy) must stay a chainable setter");

static_assert(QoS().reliability() == nros::Reliable, "default profile is reliable");
static_assert(QoS().best_effort().reliability() == nros::BestEffort, "best_effort() sets it");
static_assert(QoS().transient_local().durability() == nros::TransientLocal, "transient_local()");
static_assert(QoS().keep_all().history() == nros::KeepAll, "keep_all()");
static_assert(QoS().liveliness(nros::LivelinessManualByNode).liveliness() ==
                  nros::LivelinessManualByNode,
              "liveliness() round-trips");

// -- D5: the three windows are `Duration` --------------------------------

static_assert(std::is_same<decltype(std::declval<const QoS&>().deadline()), Duration>::value,
              "QoS::deadline() must return nros::Duration");
static_assert(std::is_same<decltype(std::declval<const QoS&>().lifespan()), Duration>::value,
              "QoS::lifespan() must return nros::Duration");
static_assert(
    std::is_same<decltype(std::declval<const QoS&>().liveliness_lease_duration()), Duration>::value,
    "QoS::liveliness_lease_duration() must return nros::Duration");

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
static_assert(nros::detail::qos_window_ms(Duration::from_nanoseconds(1)) == 1u,
              "1 ns must round UP to 1 ms, never down to the infinite sentinel");
static_assert(nros::detail::qos_window_ms(Duration::from_nanoseconds(999999)) == 1u,
              "999999 ns rounds up to 1 ms");
static_assert(nros::detail::qos_window_ms(Duration::from_nanoseconds(1000000)) == 1u,
              "exactly 1 ms is 1 ms");
static_assert(nros::detail::qos_window_ms(Duration::from_nanoseconds(1000001)) == 2u,
              "1 ms + 1 ns rounds up to 2 ms");
static_assert(nros::detail::qos_window_ms(Duration()) == 0u, "zero stays the infinite sentinel");
static_assert(nros::detail::qos_window_ms(Duration::from_nanoseconds(-5)) == 0u,
              "a negative window is the unset spelling, not a wrapped huge one");
static_assert(nros::detail::qos_window_ms(Duration::max()) == UINT32_MAX,
              "an over-long window saturates rather than wrapping short");
static_assert(QoS().deadline(Duration::from_nanoseconds(1)).deadline() ==
                  Duration::from_nanoseconds(1000000),
              "the sub-millisecond deadline is readable back as the 1 ms it became");

// -- The old spellings still compile (deprecated, not removed) ------------

static_assert(std::is_same<decltype(std::declval<const QoS&>().reliability_raw()), int>::value,
              "reliability_raw() must survive as the deprecated int getter");
static_assert(std::is_same<decltype(std::declval<const QoS&>().durability_raw()), int>::value,
              "durability_raw() must survive");
static_assert(std::is_same<decltype(std::declval<const QoS&>().history_raw()), int>::value,
              "history_raw() must survive");
static_assert(std::is_same<decltype(std::declval<const QoS&>().liveliness_raw()), int>::value,
              "liveliness_raw() must survive");
static_assert(std::is_same<decltype(std::declval<const QoS&>().deadline_ms()), uint32_t>::value,
              "deadline_ms() must survive as the deprecated ms getter");
static_assert(std::is_same<decltype(std::declval<const QoS&>().lifespan_ms()), uint32_t>::value,
              "lifespan_ms() must survive");
static_assert(
    std::is_same<decltype(std::declval<const QoS&>().liveliness_lease_ms()), uint32_t>::value,
    "liveliness_lease_ms() must survive");
static_assert(std::is_same<decltype(std::declval<QoS&>().deadline_ms(1u)), QoS&>::value,
              "deadline_ms(uint32_t) must survive as the deprecated ms setter");
static_assert(std::is_same<decltype(std::declval<QoS&>().lifespan_ms(1u)), QoS&>::value,
              "lifespan_ms(uint32_t) must survive");
static_assert(std::is_same<decltype(std::declval<QoS&>().liveliness_lease_ms(1u)), QoS&>::value,
              "liveliness_lease_ms(uint32_t) must survive");

// The deprecated members agree with the live ones — a forwarder that drifted
// from what it forwards to is the silent-mismatch failure the C half's
// `param_name_aliases.c` was written to catch, one language over.
static_assert(QoS().best_effort().reliability_raw() ==
                  static_cast<int>(QoS().best_effort().reliability()),
              "reliability_raw() and reliability() must agree");
static_assert(QoS().deadline_ms(250u).deadline() == Duration(0, 250000000u),
              "the deprecated ms setter and the Duration getter must agree");
static_assert(QoS().deadline(Duration(0, 250000000u)).deadline_ms() == 250u,
              "the Duration setter and the deprecated ms getter must agree");

// The type and the four enumerators were reachable through `QoS::` while the
// enum was a member. They still are, so no source that named one breaks.
static_assert(std::is_same<QoS::Liveliness, nros::LivelinessPolicy>::value,
              "QoS::Liveliness must stay an alias of nros::LivelinessPolicy");
static_assert(QoS::LivelinessAutomatic == nros::LivelinessAutomatic,
              "QoS::LivelinessAutomatic must stay reachable");
static_assert(QoS::LivelinessNone == nros::LivelinessNone, "QoS::LivelinessNone");
static_assert(QoS::LivelinessManualByTopic == nros::LivelinessManualByTopic,
              "QoS::LivelinessManualByTopic");
static_assert(QoS::LivelinessManualByNode == nros::LivelinessManualByNode,
              "QoS::LivelinessManualByNode");

// -- The C ABI record is unchanged ---------------------------------------
//
// The same token `deadline_ms` is a struct FIELD here and was a class METHOD
// above; the METHOD moved and the FIELD must not. A textual sweep that renamed
// both would break the by-value ABI silently (issue 0160's class).

constexpr nros_cpp_qos_t kMarshalled =
    nros::detail::qos_to_ffi(QoS()
                                 .deadline(Duration(0, 100000000u))
                                 .lifespan(Duration(1, 0))
                                 .liveliness_lease_duration(Duration(0, 5000000u))
                                 .best_effort()
                                 .transient_local()
                                 .keep_all()
                                 .liveliness(nros::LivelinessManualByNode)
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

} // namespace
