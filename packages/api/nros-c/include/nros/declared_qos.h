/* nros-c: the DECLARED QoS depth table and the compile-time check over it.
 *
 * Freestanding C11 -- no libc, no runtime cost unless a call site asks for one.
 */

/**
 * @file declared_qos.h
 * @ingroup grp_qos
 * @brief phase-454 W10 -- the C half of the declared-QoS depth check.
 *
 * # What this is
 *
 * `nros/declared_qos.hpp` gives C++ a `static_assert` that fails the BUILD when
 * the QoS depth a `NROS_SUBSCRIBE` passes disagrees with the depth the system
 * DECLARED for that topic in the contract sidecar beside the launch file. The
 * descriptor that carries those depths -- `nros_declared_qos_generated.h`, put
 * on the component's PRIVATE include path by `nano_ros_node_register()` -- is
 * pure preprocessor and language-neutral, and a C component already gets it:
 * `_nros_declared_qos_arm()` returns early only for RUST components and for
 * INTERFACE targets, and a C component is a STATIC library like a C++ one.
 *
 * What C did not have was the check. This file is it.
 *
 * # Why the table is read through a SECOND X-macro
 *
 * C++ reads `NROS_DECLARED_QOS_ROWS` by defining `NROS_DECLARED_QOS_ROW` to
 * build a `constexpr` array and searching it with a `constexpr` function. C has
 * neither, so a C lookup has to be performed by the PREPROCESSOR -- and this is
 * where the obvious translation fails:
 *
 *     #define ROW(t, tp, d)  (streq((t), q_type) && streq((tp), q_topic)) ? (d) :
 *     #define LOOKUP(q_type, q_topic)  (NROS_DECLARED_QOS_ROWS (-1))
 *
 * `q_type` inside `ROW`'s replacement list is an ordinary identifier. Macro
 * parameters are substituted only in the replacement list of the macro that
 * DECLARES them, so `LOOKUP`'s arguments never reach `ROW`, and both gcc and
 * clang report `use of undeclared identifier 'q_type'` (measured). The query
 * therefore has to travel THROUGH the list, which is exactly what the generator
 * emits beside the C++ form:
 *
 *     NROS_DECLARED_QOS_ROWS_Q(<row macro>, <queried type>, <queried topic>)
 *
 * One loop in `EntityInventory::to_declared_qos_header` writes both lists, so
 * they cannot come to say different things.
 *
 * # The string comparison is `__builtin_strcmp`, deliberately
 *
 * A `_Static_assert` needs an integer constant expression, and C gives no
 * portable way to compare two string literals in one. `"abc"[0]` is NOT an
 * integer constant expression -- gcc says *expression in static assertion is
 * not constant* and clang *not an integral constant expression* (both measured)
 * -- so a character-by-character macro chain does not work either.
 * `__builtin_strcmp` over two literals DOES fold to a constant, on gcc and
 * clang, under `-std=c11 -pedantic`, under `-ffreestanding`, under
 * `-fno-builtin`, and on the pinned `arm-none-eabi-gcc 13.2` (all measured).
 * Every C toolchain this project supports is one of those two, and a compiler
 * that is neither loses only the compile-time half: @ref NROS_DECLARED_DEPTH
 * answers @ref NROS_DECLARED_DEPTH_UNDECLARED there, every assertion holds
 * trivially, and `nros_declared_depth()` below still answers at runtime.
 *
 * # Reliability and durability too (issue 1256)
 *
 * The table also carries each subscription's declared RELIABILITY and
 * DURABILITY. @ref NROS_ASSERT_DECLARED_RELIABILITY and
 * @ref NROS_ASSERT_DECLARED_DURABILITY fail the build on a disagreement, in
 * `nros_qos_t`'s own enumerator values; a component's `nros_cpp_qos_t`, which
 * numbers reliability the C++ way, is held to the same table at REGISTRATION.
 *
 * # ABSENCE IS NOT ZERO
 *
 * A `(type, topic)` with no row is "nobody declared this endpoint", spelled
 * @ref NROS_DECLARED_DEPTH_UNDECLARED and equal to -1 rather than 0. Nothing
 * asserts against it. An image with no contract sidecar has no generated header
 * at all, gets an empty table, and compiles exactly as it did before -- which
 * is every image in this tree that has not opted in.
 *
 * # C only
 *
 * The body is `#ifndef __cplusplus`. `_Static_assert` is C11's spelling, and
 * more importantly both this header and `nros/declared_qos.hpp` would otherwise
 * define a row macro over the same list in one translation unit -- `component.h`
 * is included from C++ TUs too (`just check c` compiles it beside
 * `nros_cpp_ffi.h`). C++ has the stronger mechanism; it keeps it.
 */

#ifndef NROS_C_DECLARED_QOS_H
#define NROS_C_DECLARED_QOS_H

#ifndef __cplusplus

#include <stddef.h>

/* The per-component table, written by
 * `nros ws entity-inventory --output-header` and put on the component library's
 * include path by `nano_ros_node_register()`. Pure preprocessor -- two X-macro
 * lists plus a count -- so it can be included in any order and this header owns
 * no part of it but the reading.
 *
 * ABSENT IS THE NORMAL CASE. Every image with no contract sidecar, every
 * component whose endpoints state no `qos: { depth: }`, and every out-of-tree
 * consumer of this header has no such file and gets an empty table. */
#if defined(__has_include)
#if __has_include(<nros/nros_declared_qos_generated.h>)
#include <nros/nros_declared_qos_generated.h>
#endif
#endif

/**
 * "No depth was declared for this endpoint." NOT a depth of 0, and not the ROS
 * default of 10 -- a third state, which is the only honest answer when nobody
 * said. Negative so that no arithmetic on it can pass for a queue size.
 *
 * The same number and the same meaning as C++'s `nros::DECLARED_DEPTH_UNDECLARED`.
 */
#define NROS_DECLARED_DEPTH_UNDECLARED (-1)

/**
 * issue 1256 -- "No reliability (or durability) was declared for this
 * endpoint." The same third state as @ref NROS_DECLARED_DEPTH_UNDECLARED, for a
 * POLICY column: nothing asserts against it.
 */
#define NROS_DECLARED_POLICY_UNDECLARED (-1)

/* issue 1256 -- the policy TOKENS the generated header carries, mapped to C's
 * own enumerator values: `nros_qos_reliability_t` / `nros_qos_durability_t` in
 * `nros_generated.h`, where RELIABLE is 1 and BEST_EFFORT is 0. C++ numbers the
 * same enums the other way round, which is why the header carries tokens and
 * each language pastes them onto a prefix of its own. This header stays
 * freestanding, so the values are literals here and `<nros/subscription.h>`,
 * which sees both, `_Static_assert`s them equal to the enumerators.
 *
 * NOTE the C++ FFI's `nros_cpp_qos_t` (what a C COMPONENT hands
 * `nros_cpp_subscription_register`) uses the C++ numbering. The compile-time
 * policy macros below take `nros_qos_t` values; a component's `nros_cpp_qos_t`
 * is checked at REGISTRATION instead, by the same comparison in
 * `nros_node::declared_qos::check`, which every FFI seam calls. */
#define _NROS_DQ_C_NROS_DQ_UNDECLARED (-1)
#define _NROS_DQ_C_NROS_DQ_RELIABLE 1
#define _NROS_DQ_C_NROS_DQ_BEST_EFFORT 0
#define _NROS_DQ_C_NROS_DQ_VOLATILE 0
#define _NROS_DQ_C_NROS_DQ_TRANSIENT_LOCAL 1

/* Is the compile-time half available in this translation unit? 1 when there is
 * a table AND the compiler folds `__builtin_strcmp` over literals. Exposed so a
 * gate (and a reader) can tell "the depths agree" from "nothing was checked". */
#if defined(NROS_DECLARED_QOS_ROWS_Q) && (defined(__GNUC__) || defined(__clang__))
#define NROS_DECLARED_QOS_COMPILE_TIME 1
#else
#define NROS_DECLARED_QOS_COMPILE_TIME 0
#endif

#if NROS_DECLARED_QOS_COMPILE_TIME

/* One row of the constant-expression search. Both operands of each comparison
 * are string literals at every call site the compile-time half serves, so the
 * whole chain folds to one integer before any code is generated. */
#define _NROS_DQ_STREQ(nros_a, nros_b) (__builtin_strcmp((nros_a), (nros_b)) == 0)
#define _NROS_DQ_FIND_ROW(nros_t, nros_tp, nros_d, nros_r, nros_u, nros_q_type, nros_q_topic)      \
    (_NROS_DQ_STREQ((nros_t), (nros_q_type)) && _NROS_DQ_STREQ((nros_tp), (nros_q_topic)))         \
        ? (nros_d)                                                                                 \
        :
/* issue 1256 -- the same search, answering the RELIABILITY / DURABILITY column. */
#define _NROS_DQ_FIND_REL_ROW(nros_t, nros_tp, nros_d, nros_r, nros_u, nros_q_type, nros_q_topic)  \
    (_NROS_DQ_STREQ((nros_t), (nros_q_type)) && _NROS_DQ_STREQ((nros_tp), (nros_q_topic)))         \
        ? (_NROS_DQ_C_##nros_r)                                                                    \
        :
#define _NROS_DQ_FIND_DUR_ROW(nros_t, nros_tp, nros_d, nros_r, nros_u, nros_q_type, nros_q_topic)  \
    (_NROS_DQ_STREQ((nros_t), (nros_q_type)) && _NROS_DQ_STREQ((nros_tp), (nros_q_topic)))         \
        ? (_NROS_DQ_C_##nros_u)                                                                    \
        :

/**
 * The declared depth for `(type, topic)`, or @ref NROS_DECLARED_DEPTH_UNDECLARED.
 *
 * An integer CONSTANT EXPRESSION when both arguments are string literals, which
 * is what every call site the compile-time half serves writes. Sibling of C++'s
 * `nros::declared_depth()`, and it answers the same numbers from the same rows.
 */
#define NROS_DECLARED_DEPTH(nros_type_lit, nros_topic_lit)                                         \
    (NROS_DECLARED_QOS_ROWS_Q(_NROS_DQ_FIND_ROW, (nros_type_lit), (nros_topic_lit))                \
         NROS_DECLARED_DEPTH_UNDECLARED)

/** issue 1256 -- the declared RELIABILITY for `(type, topic)` as an
 *  `nros_qos_reliability_t` value, or @ref NROS_DECLARED_POLICY_UNDECLARED.
 *  An integer constant expression on the same terms as @ref NROS_DECLARED_DEPTH. */
#define NROS_DECLARED_RELIABILITY(nros_type_lit, nros_topic_lit)                                   \
    (NROS_DECLARED_QOS_ROWS_Q(_NROS_DQ_FIND_REL_ROW, (nros_type_lit), (nros_topic_lit))            \
         NROS_DECLARED_POLICY_UNDECLARED)

/** issue 1256 -- the declared DURABILITY as an `nros_qos_durability_t` value,
 *  or @ref NROS_DECLARED_POLICY_UNDECLARED. */
#define NROS_DECLARED_DURABILITY(nros_type_lit, nros_topic_lit)                                    \
    (NROS_DECLARED_QOS_ROWS_Q(_NROS_DQ_FIND_DUR_ROW, (nros_type_lit), (nros_topic_lit))            \
         NROS_DECLARED_POLICY_UNDECLARED)

#else /* no table, or a compiler that does not fold __builtin_strcmp */

#define NROS_DECLARED_DEPTH(nros_type_lit, nros_topic_lit) (NROS_DECLARED_DEPTH_UNDECLARED)
#define NROS_DECLARED_RELIABILITY(nros_type_lit, nros_topic_lit) (NROS_DECLARED_POLICY_UNDECLARED)
#define NROS_DECLARED_DURABILITY(nros_type_lit, nros_topic_lit) (NROS_DECLARED_POLICY_UNDECLARED)

#endif /* NROS_DECLARED_QOS_COMPILE_TIME */

/** The depth to CHECK against: the declared one, or the passed one when nothing
 *  was declared -- in which case the assertion below compares a number with
 *  itself and holds. An image that has not opted in is not an image in error. */
#define NROS_DECLARED_DEPTH_OR(nros_declared, nros_passed)                                         \
    (((nros_declared) == NROS_DECLARED_DEPTH_UNDECLARED) ? (nros_passed) : (nros_declared))

#define _NROS_DQ_CAT_(nros_a, nros_b) nros_a##nros_b
#define _NROS_DQ_CAT(nros_a, nros_b) _NROS_DQ_CAT_(nros_a, nros_b)

/**
 * Fail the BUILD when a declared depth and a passed QoS depth disagree.
 *
 * `type_lit` is the ROS or DDS-mangled type name as a string literal (a
 * generated C message header carries both spellings in the table, so either
 * works), `topic_lit` the topic as a string literal, `depth_expr` an integer
 * constant expression -- the depth the code passes -- and `topic_text` a string
 * LITERAL naming the topic for the message.
 *
 * Expands to THREE declarations, and all three are load-bearing:
 *
 *  1. the `_Static_assert`, whose message names the TOPIC -- the half that can
 *     only come from the macro, since a `_Static_assert` message must be a
 *     string literal and cannot interpolate an `int`;
 *  2. + 3. a pair of `extern char` array declarations of the SAME name, sized
 *     `declared` and `passed`. When the two agree both are `char[1]` and
 *     nothing happens. When they disagree the redeclaration conflicts, and the
 *     diagnostic names BOTH NUMBERS:
 *
 *         error: conflicting types for 'nros_declared_depth_vs_passed_at_line_42';
 *                have 'char[10]'
 *         note: previous declaration ... with type 'char[1]'
 *
 *     (gcc; clang says `redeclaration ... with a different type: 'char[10]' vs
 *     'char[1]'`.) That is the half a `_Static_assert` cannot supply. It is the
 *     C answer to what C++ does with `declared_depth_agrees<Declared, Passed>`:
 *     the numbers reach the reader through a TYPE, because that is the only
 *     part of a diagnostic a compiler prints for you.
 *
 * Together they are what a mismatch prints. Splitting them is not elegant; a
 * check that names neither the topic nor the numbers is a check nobody can act
 * on.
 *
 * The name is keyed on `__LINE__` so two subscriptions in one function do not
 * conflict with each OTHER -- which would be a false failure naming two
 * unrelated topics.
 *
 * REQUIRES `type_lit`, `topic_lit` and `depth_expr` to be constant expressions.
 * A call site whose topic or depth is built at runtime cannot be looked up at
 * compile time and takes the REGISTRATION-time check instead: the depth reaches
 * the executor through `nros_cpp_subscription_register` /
 * `nros_subscription_init_with_qos`, which is where a non-constant call site is
 * caught.
 */
#define NROS_ASSERT_DECLARED_DEPTH(nros_type_lit, nros_topic_lit, nros_depth_expr,                 \
                                   nros_topic_text)                                                \
    _Static_assert(                                                                                \
        NROS_DECLARED_DEPTH((nros_type_lit), (nros_topic_lit)) ==                                  \
                NROS_DECLARED_DEPTH_UNDECLARED ||                                                  \
            NROS_DECLARED_DEPTH((nros_type_lit), (nros_topic_lit)) == (nros_depth_expr),           \
        "nros: the QoS depth passed for topic " nros_topic_text                                    \
        " disagrees with the depth declared for that topic in the contract sidecar "               \
        "beside the launch file (<bringup>/launch/<stem>.contract.yaml, "                          \
        "contracts.sub_endpoints.<ep>.qos). Both numbers are in the "                              \
        "nros_declared_depth_vs_passed_at_line_* diagnostic beside this one -- declared "          \
        "first, passed second. Fix whichever is wrong: the contract row, or the QoS at "           \
        "the call site. Depth is a multiplier on the executor arena, so the two must "             \
        "state one number, not two.");                                                             \
    extern char _NROS_DQ_CAT(                                                                      \
        nros_declared_depth_vs_passed_at_line_,                                                    \
        __LINE__)[NROS_DECLARED_DEPTH_OR(NROS_DECLARED_DEPTH((nros_type_lit), (nros_topic_lit)),   \
                                         (nros_depth_expr)) == (nros_depth_expr)                   \
                      ? 1                                                                          \
                      : NROS_DECLARED_DEPTH_OR(                                                    \
                            NROS_DECLARED_DEPTH((nros_type_lit), (nros_topic_lit)),                \
                            (nros_depth_expr))];                                                   \
    extern char _NROS_DQ_CAT(                                                                      \
        nros_declared_depth_vs_passed_at_line_,                                                    \
        __LINE__)[NROS_DECLARED_DEPTH_OR(NROS_DECLARED_DEPTH((nros_type_lit), (nros_topic_lit)),   \
                                         (nros_depth_expr)) == (nros_depth_expr)                   \
                      ? 1                                                                          \
                      : (nros_depth_expr)];                                                        \
    (void)sizeof(_NROS_DQ_CAT(nros_declared_depth_vs_passed_at_line_, __LINE__))

/** The policy to CHECK against, on @ref NROS_DECLARED_DEPTH_OR's rule. */
#define NROS_DECLARED_POLICY_OR(nros_declared, nros_passed)                                        \
    (((nros_declared) == NROS_DECLARED_POLICY_UNDECLARED) ? (nros_passed) : (nros_declared))

/* One policy assertion, shared by the two below. The conflicting `extern char`
 * pair carries the two VALUES as array sizes, offset by one so a 0 is a legal
 * size: `char[1]` is the value 0 and `char[2]` the value 1. The identifier
 * names both the policy and that legend, because it is what the compiler
 * prints. */
#define _NROS_DQ_ASSERT_POLICY(nros_declared_expr, nros_passed_expr, nros_name, nros_message)      \
    _Static_assert((nros_declared_expr) == NROS_DECLARED_POLICY_UNDECLARED ||                      \
                       (nros_declared_expr) == (nros_passed_expr),                                 \
                   nros_message);                                                                  \
    extern char _NROS_DQ_CAT(                                                                      \
        nros_name,                                                                                 \
        __LINE__)[NROS_DECLARED_POLICY_OR((nros_declared_expr), (nros_passed_expr)) + 1];          \
    extern char _NROS_DQ_CAT(nros_name, __LINE__)[(nros_passed_expr) + 1];                         \
    (void)sizeof(_NROS_DQ_CAT(nros_name, __LINE__))

/**
 * issue 1256 -- fail the BUILD when a declared RELIABILITY and the passed one
 * disagree. `nros_rel_expr` is an `nros_qos_reliability_t` constant expression
 * -- spell the enumerator (`NROS_QOS_RELIABILITY_BEST_EFFORT`): a member of a
 * `const` struct such as `NROS_QOS_SENSOR_DATA.reliability` is not an integer
 * constant expression in C. Same four-argument shape as
 * @ref NROS_ASSERT_DECLARED_DEPTH.
 *
 * Not about memory: an incompatible-QoS match never delivers, so a call site
 * contradicting its contract is a link that silently carries nothing.
 */
#define NROS_ASSERT_DECLARED_RELIABILITY(nros_type_lit, nros_topic_lit, nros_rel_expr,             \
                                         nros_topic_text)                                          \
    _NROS_DQ_ASSERT_POLICY(                                                                        \
        NROS_DECLARED_RELIABILITY((nros_type_lit), (nros_topic_lit)), (nros_rel_expr),             \
        nros_declared_reliability_vs_passed__1_best_effort_2_reliable__at_line_,                   \
        "nros: the QoS reliability passed for topic " nros_topic_text                              \
        " disagrees with the reliability declared for that topic in the "                          \
        "contract sidecar (<stem>.contract.yaml). Both values are in the "                         \
        "nros_declared_reliability_vs_passed_* diagnostic beside this one -- "                     \
        "declared first, passed second.")

/**
 * issue 1256 -- the same for DURABILITY. `nros_dur_expr` is an
 * `nros_qos_durability_t` constant expression.
 */
#define NROS_ASSERT_DECLARED_DURABILITY(nros_type_lit, nros_topic_lit, nros_dur_expr,              \
                                        nros_topic_text)                                           \
    _NROS_DQ_ASSERT_POLICY(                                                                        \
        NROS_DECLARED_DURABILITY((nros_type_lit), (nros_topic_lit)), (nros_dur_expr),              \
        nros_declared_durability_vs_passed__1_volatile_2_transient_local__at_line_,                \
        "nros: the QoS durability passed for topic " nros_topic_text                               \
        " disagrees with the durability declared for that topic in the contract sidecar "          \
        "(<stem>.contract.yaml). Both values are in the "                                          \
        "nros_declared_durability_vs_passed_* diagnostic beside this one -- declared first, "      \
        "passed second.")

/** `strcmp(a, b) == 0`, spelled here so this header needs no `<string.h>` --
 *  which a freestanding C implementation is not required to provide. */
static inline int nros_declared_qos_streq(const char* nros_a, const char* nros_b) {
    if (nros_a == NULL || nros_b == NULL) {
        return nros_a == nros_b;
    }
    while (*nros_a != '\0' && *nros_a == *nros_b) {
        ++nros_a;
        ++nros_b;
    }
    return *nros_a == *nros_b;
}

/**
 * The declared depth for `(type, topic)` at RUNTIME, or
 * @ref NROS_DECLARED_DEPTH_UNDECLARED.
 *
 * The same rows and the same answer as @ref NROS_DECLARED_DEPTH; this is the
 * form for a topic that is not a constant expression, and for a caller that
 * wants to REPORT rather than refuse to build. No storage: the table is an
 * `if` chain over string literals, so a TU that never calls this emits nothing.
 */
static inline int nros_declared_depth(const char* nros_type, const char* nros_topic) {
#if defined(NROS_DECLARED_QOS_ROWS_Q)
#define _NROS_DQ_RUNTIME_ROW(nros_t, nros_tp, nros_d, nros_r, nros_u, nros_q_type, nros_q_topic)   \
    if (nros_declared_qos_streq((nros_t), (nros_q_type)) &&                                        \
        nros_declared_qos_streq((nros_tp), (nros_q_topic))) {                                      \
        return (nros_d);                                                                           \
    }
    NROS_DECLARED_QOS_ROWS_Q(_NROS_DQ_RUNTIME_ROW, nros_type, nros_topic)
#undef _NROS_DQ_RUNTIME_ROW
#else
    (void)nros_type;
    (void)nros_topic;
#endif
    return NROS_DECLARED_DEPTH_UNDECLARED;
}

/**
 * issue 1256 -- the declared RELIABILITY at RUNTIME, as an
 * `nros_qos_reliability_t` value, or @ref NROS_DECLARED_POLICY_UNDECLARED.
 * The same rows as @ref NROS_DECLARED_RELIABILITY.
 */
static inline int nros_declared_reliability(const char* nros_type, const char* nros_topic) {
#if defined(NROS_DECLARED_QOS_ROWS_Q)
#define _NROS_DQ_RUNTIME_REL_ROW(nros_t, nros_tp, nros_d, nros_r, nros_u, nros_q_type,             \
                                 nros_q_topic)                                                     \
    if (nros_declared_qos_streq((nros_t), (nros_q_type)) &&                                        \
        nros_declared_qos_streq((nros_tp), (nros_q_topic))) {                                      \
        return (_NROS_DQ_C_##nros_r);                                                              \
    }
    NROS_DECLARED_QOS_ROWS_Q(_NROS_DQ_RUNTIME_REL_ROW, nros_type, nros_topic)
#undef _NROS_DQ_RUNTIME_REL_ROW
#else
    (void)nros_type;
    (void)nros_topic;
#endif
    return NROS_DECLARED_POLICY_UNDECLARED;
}

/**
 * issue 1256 -- the declared DURABILITY at RUNTIME, as an
 * `nros_qos_durability_t` value, or @ref NROS_DECLARED_POLICY_UNDECLARED.
 */
static inline int nros_declared_durability(const char* nros_type, const char* nros_topic) {
#if defined(NROS_DECLARED_QOS_ROWS_Q)
#define _NROS_DQ_RUNTIME_DUR_ROW(nros_t, nros_tp, nros_d, nros_r, nros_u, nros_q_type,             \
                                 nros_q_topic)                                                     \
    if (nros_declared_qos_streq((nros_t), (nros_q_type)) &&                                        \
        nros_declared_qos_streq((nros_tp), (nros_q_topic))) {                                      \
        return (_NROS_DQ_C_##nros_u);                                                              \
    }
    NROS_DECLARED_QOS_ROWS_Q(_NROS_DQ_RUNTIME_DUR_ROW, nros_type, nros_topic)
#undef _NROS_DQ_RUNTIME_DUR_ROW
#else
    (void)nros_type;
    (void)nros_topic;
#endif
    return NROS_DECLARED_POLICY_UNDECLARED;
}

/**
 * Does `depth` agree with what this component's contract declared for
 * `(type, topic)`? Non-zero when it may proceed -- which includes the
 * UNDECLARED case, because absence is not a disagreement.
 *
 * For a call site that cannot be checked at compile time. It answers; it does
 * not print. A caller that wants the two numbers in a diagnostic reads
 * `nros_declared_depth()` for the declared one and already holds the other.
 */
static inline int nros_declared_depth_agrees(const char* nros_type, const char* nros_topic,
                                             int nros_depth) {
    const int nros_declared = nros_declared_depth(nros_type, nros_topic);
    return nros_declared == NROS_DECLARED_DEPTH_UNDECLARED || nros_declared == nros_depth;
}

#endif /* !__cplusplus */

#endif /* NROS_C_DECLARED_QOS_H */
