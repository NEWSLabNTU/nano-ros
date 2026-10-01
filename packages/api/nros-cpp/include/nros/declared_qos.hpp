// nros-cpp: the DECLARED QoS depth table and the compile-time check over it
// Freestanding C++ -- no STL, no runtime cost unless a call site asks for one.

/**
 * @file declared_qos.hpp
 * @ingroup grp_qos
 * @brief phase-403 step 2 -- the QoS history depth an image DECLARED, reaching the
 *        compiler.
 *
 * # The problem
 *
 * The executor arena charges each subscription
 * `sizeof(entry) + (depth + 1) * bound + (depth + 1) * 8`, so the QoS history
 * DEPTH is a multiplier on the type's serialized bound. Measured on
 * mr-canhubk344 at ten subscriptions: 86108 bytes at the ROS default depth 10,
 * 24516 at depth 1. A build that wants to derive that number has to know the
 * depth, and it has to know it BEFORE the component TU is compiled -- which
 * rules out reading it off the running image.
 *
 * Defaulting is the worst option available. The ROS default IS 10, so assuming
 * it inflates an image that meant 1 by a factor of ten; assuming 1 UNDER-sizes
 * one that took the default, which is the unsafe direction. So the depth is
 * DECLARED -- once per system, in the contract sidecar beside the launch file
 * that runs the node (`<bringup>/launch/<stem>.contract.yaml`):
 *
 *     topics:
 *       /chatter:
 *         type: std_msgs/msg/Int32
 *         sub: [/listener/chatter]
 *     contracts:
 *       sub_endpoints:
 *         /listener/chatter:
 *           qos: { depth: 1 }
 *
 * The row is keyed on the TOPIC, which is what a call site writes, and not on
 * the endpoint ref the contract addresses it by (issue 1084).
 *
 * # Two authoring modes, and only one number
 *
 * | the code writes                            | the contract says        |
 * | ------------------------------------------ | ------------------------ |
 * | `NROS_SUBSCRIBE(M, m, "/t", nros::QoS(1))` | `qos: { depth: 1 }`      |
 * | `NROS_SUBSCRIBE(M, m, "/t")`               | `qos: { depth: 1 }` fills in |
 *
 * Both are legal. What is not legal is the two stating DIFFERENT numbers, and
 * that is caught HERE, at compile time: the declaration is emitted as a
 * `constexpr` table, `NROS_SUBSCRIBE` looks its own `(type, topic)` up in it,
 * and a disagreement is a `static_assert` failure naming the topic and both
 * depths. Nothing is checked at runtime on that path and nothing costs a byte.
 *
 * # Reliability and durability too (issue 1256)
 *
 * A contract can also state `reliability:` and `durability:` for a
 * subscription, and the table carries both beside the depth. A disagreement
 * there is not a sizing fault but an INTEROP one -- an incompatible-QoS match
 * never delivers -- so it fails the build the same way:
 * @ref NROS_ASSERT_DECLARED_QOS checks all three columns (it is what
 * `NROS_SUBSCRIBE`'s four-argument form asserts), and the three-argument form
 * fills every declared column in. A column nobody stated is UNDECLARED and
 * asserts nothing.
 *
 * # What this deliberately does NOT do
 *
 * It does not read a QoS out of C++ source text. An earlier sketch had a lint
 * parsing `NROS_SUBSCRIBE` call sites for a QoS literal; that only ever sees
 * the literal forms and silently passes everything else. Comparing the actual
 * `QoS` OBJECT -- whatever expression produced it, as long as it is constant --
 * is exact.
 *
 * # ABSENCE IS NOT ZERO
 *
 * A `(type, topic)` with no row is "nobody declared this endpoint", spelled
 * @ref nros::DECLARED_DEPTH_UNDECLARED and equal to -1 rather than 0. Nothing
 * asserts against it, `NROS_SUBSCRIBE` without a QoS keeps its historical
 * `QoS::default_profile()`, and anything that would SIZE from depth must refuse
 * rather than default. An image that has not opted in is not an image in error.
 *
 * # The C++14 subset, deliberately
 *
 * Not the project minimum — that is C++17 since issue 1118. This is the
 * stricter property `just check cpp` still enforces: it parses every header
 * here with `-std=c++14 -ffreestanding`,
 * so the table is a `constexpr` array and the search is a recursive `constexpr`
 * function. Both are constant-evaluated; the array is `constexpr` at namespace
 * scope, so a TU that only ever uses it in a constant expression emits no
 * storage for it at all.
 */

#ifndef NROS_CPP_DECLARED_QOS_HPP
#define NROS_CPP_DECLARED_QOS_HPP

#include <stddef.h>

// The per-component table, written by
// `nros ws entity-inventory --output-header` and put on the component library's
// include path by `nano_ros_node_register()`. It is pure preprocessor -- an
// X-macro list plus a count -- so it can be included in any order and this
// header owns the one definition of the table's TYPE.
//
// ABSENT IS THE NORMAL CASE. Every image with no contract sidecar, every
// component whose endpoints state no `qos: { depth: }`, and every out-of-tree
// consumer of this header has no such file, gets an empty table, and compiles
// exactly as before.
#if defined(__has_include)
#if __has_include(<nros/nros_declared_qos_generated.h>)
#include <nros/nros_declared_qos_generated.h>
#endif
#endif

namespace nros {

/// "No depth was declared for this endpoint." NOT a depth of 0, and not the ROS
/// default of 10 -- a third state, which is the only honest answer when nobody
/// said. Negative so that no arithmetic on it can pass for a queue size.
constexpr int DECLARED_DEPTH_UNDECLARED = -1;

/// issue 1256 -- "No reliability (or durability) was declared for this
/// endpoint." The same third state as @ref DECLARED_DEPTH_UNDECLARED, for a
/// POLICY column: nothing asserts against it.
constexpr int DECLARED_POLICY_UNDECLARED = -1;

// issue 1256 -- the policy TOKENS the generated header carries, mapped to this
// language's own enumerator ordinals (`nros::ReliabilityPolicy` /
// `nros::DurabilityPolicy` in `nros/qos.hpp`). The header carries TOKENS rather
// than numbers because C numbers the same enums differently
// (`NROS_QOS_RELIABILITY_RELIABLE` is 1, `nros::Reliable` is 0), so each
// language pastes the token onto a prefix of its own. This header cannot
// include `nros/qos.hpp` -- that one needs per-build generated config headers,
// and this one must stay freestanding -- so the ordinals are literals here and
// `nros/component.hpp`, which sees both, `static_assert`s them equal to the
// enumerators. A drift is a compile error in every C++ image, not a silent
// mismatch.
#define _NROS_DQ_CPP_NROS_DQ_UNDECLARED (-1)
#define _NROS_DQ_CPP_NROS_DQ_RELIABLE 0
#define _NROS_DQ_CPP_NROS_DQ_BEST_EFFORT 1
#define _NROS_DQ_CPP_NROS_DQ_VOLATILE 0
#define _NROS_DQ_CPP_NROS_DQ_TRANSIENT_LOCAL 1

namespace declared_qos {

/// One row of the declared table. `type` is what a generated message class
/// carries as `M::TYPE_NAME`; `topic` is the endpoint name. Each of the three
/// columns is the declared value or its UNDECLARED sentinel (-1): a row exists
/// when ANY column is declared, so any one of them may be silent.
struct Entry {
    const char* type;
    const char* topic;
    int depth;
    /// `nros::ReliabilityPolicy` ordinal, or @ref DECLARED_POLICY_UNDECLARED.
    int reliability;
    /// `nros::DurabilityPolicy` ordinal, or @ref DECLARED_POLICY_UNDECLARED.
    int durability;
};

#if defined(NROS_DECLARED_QOS_ROWS)
#define NROS_DECLARED_QOS_ROW(nros_type, nros_topic, nros_depth, nros_rel, nros_dur)               \
    {(nros_type), (nros_topic), (nros_depth), _NROS_DQ_CPP_##nros_rel, _NROS_DQ_CPP_##nros_dur},
/// The declared endpoints of THIS component. The trailing sentinel keeps the
/// array non-empty, which C++ requires; `COUNT` is what the search reads, so
/// the sentinel is never examined.
constexpr Entry TABLE[] = {
    NROS_DECLARED_QOS_ROWS{nullptr, nullptr, ::nros::DECLARED_DEPTH_UNDECLARED,
                           ::nros::DECLARED_POLICY_UNDECLARED, ::nros::DECLARED_POLICY_UNDECLARED}};
#undef NROS_DECLARED_QOS_ROW
constexpr size_t COUNT = NROS_DECLARED_QOS_ROW_COUNT;
#else
/// No generated table on the include path. One sentinel row and a COUNT of
/// ZERO -- which is "the table holds nothing", a different claim from "this
/// endpoint was declared depth zero", and the reason `COUNT` exists separately
/// from the array's length.
constexpr Entry TABLE[] = {{nullptr, nullptr, ::nros::DECLARED_DEPTH_UNDECLARED,
                            ::nros::DECLARED_POLICY_UNDECLARED,
                            ::nros::DECLARED_POLICY_UNDECLARED}};
constexpr size_t COUNT = 0;
#endif

} // namespace declared_qos

namespace detail {

/// `strcmp(a, b) == 0` as a C++14 constant expression. Recursive rather than
/// looped because this header is parsed at `-std=c++14`, where a `constexpr`
/// function body is a single return statement.
constexpr bool declared_qos_streq(const char* a, const char* b) {
    return (*a == *b) ? ((*a == '\0') ? true : declared_qos_streq(a + 1, b + 1)) : false;
}

/// Linear search of the table: the row for `(type, topic)`, or `nullptr`. Tens
/// of entries evaluated at compile time; the cost is a constexpr loop nobody
/// notices and no runtime storage at all.
constexpr const ::nros::declared_qos::Entry*
declared_qos_row(const ::nros::declared_qos::Entry* rows, size_t n, const char* type,
                 const char* topic) {
    return (n == 0) ? nullptr
                    : ((declared_qos_streq(rows[0].type, type) &&
                        declared_qos_streq(rows[0].topic, topic))
                           ? rows
                           : declared_qos_row(rows + 1, n - 1, type, topic));
}

/// The declared DEPTH for `(type, topic)` in `rows`, or the undeclared sentinel.
constexpr int declared_qos_find(const ::nros::declared_qos::Entry* rows, size_t n, const char* type,
                                const char* topic) {
    return (declared_qos_row(rows, n, type, topic) == nullptr)
               ? ::nros::DECLARED_DEPTH_UNDECLARED
               : declared_qos_row(rows, n, type, topic)->depth;
}

/// The compile-time verdict, as a TEMPLATE so the two numbers reach the
/// diagnostic.
///
/// A `static_assert` message must be a string literal, so it cannot interpolate
/// two `constexpr int`s. Template ARGUMENTS can be printed by the compiler,
/// though, and both gcc and clang name the instantiation:
///
///     In instantiation of 'struct nros::detail::declared_depth_agrees<10, 1>'
///     error: static assertion failed: nros: this subscription's QoS depth ...
///     note: '(10 == 1)' evaluates to false
///
/// That is why the assertion lives INSIDE the template rather than at the call
/// site: at the call site the operands are `constexpr` function calls, and
/// neither compiler reduces those to numbers in the diagnostic. The call site
/// contributes the half a template cannot -- the TOPIC, which is a string and
/// so can only reach the message through macro stringification.
template <int Declared, int Passed> struct declared_depth_agrees {
    static_assert(Declared == Passed,
                  "nros: this subscription's QoS depth disagrees with the depth its system "
                  "DECLARED for that topic in the contract sidecar beside the launch file "
                  "(<bringup>/launch/<stem>.contract.yaml, contracts.sub_endpoints.<ep>.qos). "
                  "The two numbers are the template arguments of "
                  "nros::detail::declared_depth_agrees<declared, passed> named just above -- "
                  "declared first, passed second. The TOPIC is named by the assertion beside "
                  "this one. Fix whichever is wrong: the contract row, or the QoS at the "
                  "call site. Depth is a multiplier on the arena, so the two must agree.");
    static constexpr bool value = (Declared == Passed);
};

/// The depth to CHECK against. When nothing was declared there is nothing to
/// disagree with, so the passed depth is compared with itself and the assertion
/// above holds trivially -- an image that has not opted in is not in error.
constexpr int declared_depth_or(int declared, int passed) {
    return (declared == ::nros::DECLARED_DEPTH_UNDECLARED) ? passed : declared;
}

/// issue 1256 -- the reliability verdict, shaped like `declared_depth_agrees`
/// for the same reason: the two ORDINALS reach the reader as template
/// arguments, the one part of a diagnostic a compiler prints for you.
template <int Declared, int Passed> struct declared_reliability_agrees {
    static_assert(Declared == Passed,
                  "nros: this subscription's QoS RELIABILITY disagrees with the reliability its "
                  "system DECLARED for that topic in the contract sidecar "
                  "(<bringup>/launch/<stem>.contract.yaml, contracts.sub_endpoints.<ep>.qos). "
                  "The two values are the template arguments of "
                  "nros::detail::declared_reliability_agrees<declared, passed> named just above "
                  "-- declared first, passed second; 0 is reliable, 1 is best_effort "
                  "(nros::ReliabilityPolicy). An incompatible-QoS match never delivers, so the "
                  "two must agree: fix the contract row or the QoS at the call site.");
    static constexpr bool value = (Declared == Passed);
};

/// issue 1256 -- the durability verdict. See `declared_reliability_agrees`.
template <int Declared, int Passed> struct declared_durability_agrees {
    static_assert(Declared == Passed,
                  "nros: this subscription's QoS DURABILITY disagrees with the durability its "
                  "system DECLARED for that topic in the contract sidecar "
                  "(<bringup>/launch/<stem>.contract.yaml, contracts.sub_endpoints.<ep>.qos). "
                  "The two values are the template arguments of "
                  "nros::detail::declared_durability_agrees<declared, passed> named just above "
                  "-- declared first, passed second; 0 is volatile, 1 is transient_local "
                  "(nros::DurabilityPolicy). Fix the contract row or the QoS at the call site.");
    static constexpr bool value = (Declared == Passed);
};

/// The policy to CHECK against, on `declared_depth_or`'s rule: nothing
/// declared compares the passed value with itself.
constexpr int declared_policy_or(int declared, int passed) {
    return (declared == ::nros::DECLARED_POLICY_UNDECLARED) ? passed : declared;
}

} // namespace detail

/// The declared depth for `(type, topic)`, or @ref DECLARED_DEPTH_UNDECLARED.
///
/// `constexpr`, so it answers in a `static_assert` when both arguments are
/// constant expressions -- which they are at every call site whose topic is a
/// string literal. It is an ordinary function too, so the boot-time fallback in
/// `Node::check_declared_depth` calls the SAME lookup for a topic that
/// is only known at runtime. One implementation, two evaluation times.
constexpr int declared_depth(const char* type, const char* topic) {
    return detail::declared_qos_find(declared_qos::TABLE, declared_qos::COUNT, type, topic);
}

/// issue 1256 -- the declared RELIABILITY for `(type, topic)`, as a
/// `nros::ReliabilityPolicy` ordinal, or @ref DECLARED_POLICY_UNDECLARED.
/// `constexpr` on the same terms as @ref declared_depth.
constexpr int declared_reliability(const char* type, const char* topic) {
    return (detail::declared_qos_row(declared_qos::TABLE, declared_qos::COUNT, type, topic) ==
            nullptr)
               ? DECLARED_POLICY_UNDECLARED
               : detail::declared_qos_row(declared_qos::TABLE, declared_qos::COUNT, type, topic)
                     ->reliability;
}

/// issue 1256 -- the declared DURABILITY for `(type, topic)`, as a
/// `nros::DurabilityPolicy` ordinal, or @ref DECLARED_POLICY_UNDECLARED.
constexpr int declared_durability(const char* type, const char* topic) {
    return (detail::declared_qos_row(declared_qos::TABLE, declared_qos::COUNT, type, topic) ==
            nullptr)
               ? DECLARED_POLICY_UNDECLARED
               : detail::declared_qos_row(declared_qos::TABLE, declared_qos::COUNT, type, topic)
                     ->durability;
}

} // namespace nros

/// Fail the BUILD when a declared depth and a passed QoS depth disagree.
///
/// `type_name` is `M::TYPE_NAME`, `topic_expr` the topic, `qos_expr` the QoS
/// object, and `topic_text` a string LITERAL naming the topic for the message
/// (`NROS_SUBSCRIBE` passes `#topic`, so it reads as the call site wrote it).
///
/// Expands to two statements, and both are load-bearing:
///
///  1. the `static_assert`, whose message names the TOPIC -- the half that can
///     only come from the macro, since a string cannot be a template argument
///     in C++17;
///  2. a `sizeof` that instantiates `declared_depth_agrees<declared, passed>`,
///     whose diagnostic names both NUMBERS -- the half that can only come from
///     a template, since a `static_assert` message cannot interpolate an `int`.
///
/// Together they are what a mismatch prints. Splitting them is not elegant; it
/// is the price of `static_assert` taking a literal, and a check that names
/// neither the topic nor the numbers is a check nobody can act on.
///
/// REQUIRES `topic_expr` to be a constant expression -- in practice, a string
/// literal, which is what every call site in this tree uses. A call site whose
/// topic is built at runtime cannot be looked up at compile time and gets the
/// boot-time check in `Node::check_declared_depth` instead; see
/// `NROS_SUBSCRIBE_DYNAMIC`.
#define NROS_ASSERT_DECLARED_DEPTH(type_name, topic_expr, qos_expr, topic_text)                    \
    static_assert(::nros::declared_depth((type_name), (topic_expr)) ==                             \
                          ::nros::DECLARED_DEPTH_UNDECLARED ||                                     \
                      ::nros::declared_depth((type_name), (topic_expr)) == (qos_expr).depth(),     \
                  "nros: the QoS depth passed for topic " topic_text                               \
                  " disagrees with the depth declared for that topic in the contract "             \
                  "sidecar (<stem>.contract.yaml). Both numbers are in the "                       \
                  "declared_depth_agrees<declared, passed> diagnostic beside this one.");          \
    (void)sizeof(::nros::detail::declared_depth_agrees<                                            \
                 ::nros::detail::declared_depth_or(                                                \
                     ::nros::declared_depth((type_name), (topic_expr)), (qos_expr).depth()),       \
                 (qos_expr).depth()>)

/// issue 1256 -- fail the BUILD when a declared RELIABILITY and the passed
/// QoS's disagree. Same four arguments and the same two-statement shape as
/// @ref NROS_ASSERT_DECLARED_DEPTH; `qos_expr` must have a `constexpr
/// reliability()` (an `nros::QoS` does).
///
/// This disagreement is not about memory. An incompatible-QoS match never
/// delivers -- a RELIABLE subscription is never matched to a BEST_EFFORT
/// publisher -- so a call site that contradicts its contract is a link that
/// silently carries nothing, and the build is the cheapest place to say so.
#define NROS_ASSERT_DECLARED_RELIABILITY(type_name, topic_expr, qos_expr, topic_text)              \
    static_assert(::nros::declared_reliability((type_name), (topic_expr)) ==                       \
                          ::nros::DECLARED_POLICY_UNDECLARED ||                                    \
                      ::nros::declared_reliability((type_name), (topic_expr)) ==                   \
                          static_cast<int>((qos_expr).reliability()),                              \
                  "nros: the QoS reliability passed for topic " topic_text                         \
                  " disagrees with the reliability declared for that topic in the contract "       \
                  "sidecar (<stem>.contract.yaml). Both values are in the "                        \
                  "declared_reliability_agrees<declared, passed> diagnostic beside this one.");    \
    (void)sizeof(::nros::detail::declared_reliability_agrees<                                      \
                 ::nros::detail::declared_policy_or(                                               \
                     ::nros::declared_reliability((type_name), (topic_expr)),                      \
                     static_cast<int>((qos_expr).reliability())),                                  \
                 static_cast<int>((qos_expr).reliability())>)

/// issue 1256 -- the same for DURABILITY. `qos_expr` must have a `constexpr
/// durability()`.
#define NROS_ASSERT_DECLARED_DURABILITY(type_name, topic_expr, qos_expr, topic_text)               \
    static_assert(::nros::declared_durability((type_name), (topic_expr)) ==                        \
                          ::nros::DECLARED_POLICY_UNDECLARED ||                                    \
                      ::nros::declared_durability((type_name), (topic_expr)) ==                    \
                          static_cast<int>((qos_expr).durability()),                               \
                  "nros: the QoS durability passed for topic " topic_text                          \
                  " disagrees with the durability declared for that topic in the contract "        \
                  "sidecar (<stem>.contract.yaml). Both values are in the "                        \
                  "declared_durability_agrees<declared, passed> diagnostic beside this one.");     \
    (void)sizeof(::nros::detail::declared_durability_agrees<                                       \
                 ::nros::detail::declared_policy_or(                                               \
                     ::nros::declared_durability((type_name), (topic_expr)),                       \
                     static_cast<int>((qos_expr).durability())),                                   \
                 static_cast<int>((qos_expr).durability())>)

/// issue 1256 -- every declared column at once: depth, reliability and
/// durability. What `NROS_SUBSCRIBE`'s four-argument form asserts, and what a
/// call site that registers through `nros::bind_subscription` writes beside it
/// when it states its QoS (see `examples/workspaces/cpp/src/listener_pkg`).
#define NROS_ASSERT_DECLARED_QOS(type_name, topic_expr, qos_expr, topic_text)                      \
    NROS_ASSERT_DECLARED_DEPTH(type_name, topic_expr, qos_expr, topic_text);                       \
    NROS_ASSERT_DECLARED_RELIABILITY(type_name, topic_expr, qos_expr, topic_text);                 \
    NROS_ASSERT_DECLARED_DURABILITY(type_name, topic_expr, qos_expr, topic_text)

#endif // NROS_CPP_DECLARED_QOS_HPP
