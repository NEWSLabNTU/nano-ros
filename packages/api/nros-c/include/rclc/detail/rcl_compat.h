/**
 * @file rcl_compat.h
 * @brief What is LEFT of the rcl compat layer after phase-417 stage 6.
 *
 * RFC-0089 settled on 2026-09-04 that **the C API takes rcl's spellings**,
 * because the goal is drop-in replacement and a ported file's line is rcl's
 * line. The migration it prescribes is ALIAS first, REPLACE later. Stage 5 was
 * the alias step and this header held twelve `static inline` forwarders;
 * **stage 6 renamed the entry points themselves, so all twelve became IDENTITY
 * and were deleted** — see section 3 for the list and for where each one lives
 * now.
 *
 * That is RFC-0089's "dissolves by construction": a shim exists to bridge two
 * spellings, and when there is one spelling it has nothing to bridge. The
 * deletion is not a judgement call either — an identity `static inline` beside
 * the non-static declaration in `<rclc/detail/nros_generated.h>` is a hard
 * `error: static declaration of '...' follows non-static declaration`.
 *
 * ## What does NOT dissolve, and why that is fine
 *
 * TWO things, and neither is a spelling:
 *
 * 1. **The `RCL_RET_*` value MAPPING (section 1).** RFC-0089 forbids
 *    renumbering `rcl_ret_t` to rcl's values — that would silently flip the
 *    meaning of every stored return code across three FFI seams. So
 *    `RCL_RET_TIMEOUT` does not go away; it becomes the NAME of our constant,
 *    with our value. A ported `if (ret == RCL_RET_TIMEOUT)` is correct; a
 *    program that hardcodes `if (ret == 2)` is not, and never was.
 * 2. **The handle TYPEDEFS (section 2).** Our structs are still
 *    `struct rcl_node_t` and friends, so `rcl_node_t` is still an alias
 *    rather than the name itself. These retire with the TYPE rename, which is
 *    a separate step from the entry-point rename this file records.
 *
 *     #include <rclc/detail/rcl_compat.h>   // opt-in; nothing force-includes it
 *
 * ## Scope
 *
 * Freestanding C: no allocator, no libc beyond what `<rclc/types.h>` already
 * uses. This header adds no symbol to the link — every remaining item is a
 * typedef or a macro.
 */

#ifndef NROS_RCL_COMPAT_H
#define NROS_RCL_COMPAT_H

/* ── Collision guard ──────────────────────────────────────────────────────
 * The real rcl and this header define the same names with DIFFERENT VALUES
 * (see the mapping table below), so a translation unit holding both would
 * silently take whichever won the include race. Refuse instead of redefining.
 *
 * One-way only: C cannot detect an `<rcl/types.h>` included AFTER this file.
 * If you are linking real rcl, you do not want this header at all.
 *
 * The whole body sits in the `#else` arm so that a collision produces exactly
 * ONE diagnostic. Letting it fall through would bury the message under a
 * cascade of "RCL_RET_OK redefined" warnings, which is the shape of an error
 * a reader skims past. */
#if defined(RCL__TYPES_H_)
#error "<rclc/detail/rcl_compat.h> collides with the real <rcl/types.h>: both define \
RCL_RET_* and the VALUES differ (nano-ros ERROR/TIMEOUT/INVALID_ARGUMENT/NOT_INIT \
are -1/-2/-3/-7, rcl's are 1/2/11/101). Include one. If you are building against \
a real ROS 2 rcl, drop this header."
#elif defined(RMW_RET_OK) || defined(RMW__RET_TYPES_H_)
#error "<rclc/detail/rcl_compat.h> collides with the real <rmw/ret_types.h>: both define \
RMW_RET_* and the VALUES differ. Include one."
#else

#include "rclc/types.h"

/* ═══════════════════════════════════════════════════════════════════════════
 * 1. Return codes — the SPELLING is adopted, the VALUES are NOT
 * ═══════════════════════════════════════════════════════════════════════════
 *
 * `rcl_ret_t`'s own doc comment USED TO claim it was "Compatible with
 * rcl_ret_t for familiarity". **It is not**, and W5.b corrected the comment as
 * well as adding the mapping below — a doc comment is the only thing a caller
 * can read at the call site, so leaving it while shipping a header that
 * contradicts it would have kept the defect and added a second copy of it.
 * `<rclc/detail/nros_generated.h>`'s `rcl_ret_t` now leads with "NOT value-compatible
 * with `rcl_ret_t` — only `OK` (0) agrees" and carries this same table. (No
 * line number: the generated header moves, and a stale `:840` is how a citation
 * stops being checkable.) Only OK agrees:
 *
 *     code                ours   rcl   rcl's source
 *     OK                     0     0   RMW_RET_OK
 *     ERROR                 -1     1   RMW_RET_ERROR
 *     TIMEOUT               -2     2   RMW_RET_TIMEOUT
 *     UNSUPPORTED          -16     3   RMW_RET_UNSUPPORTED
 *     INVALID_ARGUMENT      -3    11   RMW_RET_INVALID_ARGUMENT
 *     NOT_INIT              -7   101   RCL_RET_NOT_INIT
 *
 * (read from `/opt/ros/humble/include/rmw/rmw/ret_types.h:29-38` and
 * `rcl/rcl/types.h:26-41`.)
 *
 * So this header MAPS the constants onto ours. It does NOT renumber
 * `rcl_ret_t` — RFC-0089 records this as the one place where taking rcl's
 * spelling must not mean taking rcl's values, because a stored return code
 * crosses three FFI seams (C, C++, the Rust bindings) and renumbering would
 * flip the meaning of every one of them silently. A ported
 * `if (ret == RCL_RET_TIMEOUT)` is true exactly when ours returned a timeout,
 * which is the property that matters; the integer behind it is not.
 *
 * ── Which constants exist here, and why the rest do not ───────────────────
 *
 * Only codes a nano-ros entry point actually returns are defined. That is not
 * laziness: **a constant we define but never return is a branch that compiles
 * and is never taken**, which is precisely the silent difference the
 * compile-or-conform rule forbids. Leaving it undefined makes the ported line
 * fail to compile, which is the loudness the rule requires.
 *
 * Not defined, and the reason:
 *
 * - `RCL_RET_BAD_ALLOC` — rcl means "an allocation failed, and freeing may
 *   let you retry". The nearest nano-ros code is `RCLC_RET_FULL`, which means
 *   "a compile-time arena is full; rebuild it larger". Same shape, different
 *   REMEDY, so a ported handler would do the wrong thing while compiling
 *   cleanly. Refused rather than approximated.
 * - `RCL_RET_SUBSCRIPTION_TAKE_FAILED` / `_CLIENT_TAKE_FAILED` /
 *   `_SERVICE_TAKE_FAILED` — our take entry points report "nothing available"
 *   as a byte COUNT (`int32_t`), not as an `rcl_ret_t`; there is no code to
 *   map onto.
 * - `RCL_RET_ALREADY_INIT`, `RCL_RET_ALREADY_SHUTDOWN`,
 *   `RCL_RET_*_INVALID`, `RCL_RET_TIMER_CANCELED`, `RCL_RET_WAIT_SET_*`,
 *   `RCL_RET_MISMATCHED_RMW_ID`, the argument-parsing 1XXX block — no
 *   nano-ros entry point returns a distinguishable equivalent. Two of these
 *   had two equally plausible nano-ros targets each, which is itself the
 *   signal not to guess.
 */

/* `rcl_ret_t` itself is ours since phase-492 (the generated header defines
 * it, an `int`); the width guard is asserted below. */
/** Upstream `rcl_ret_t` IS `rmw_ret_t`, and ported rcl code mixes the two
 *  freely, so the alias exists here too. Nothing else of rmw is provided —
 *  this is the rcl compat header, not an rmw one. */
typedef rcl_ret_t rmw_ret_t;

/** Success. The one code where ours and rcl's VALUES agree (both 0). */
#define RMW_RET_OK RCL_RET_OK
/** Generic error. rcl's value is 1; ours is -1. Mapped, not renumbered. */
#define RMW_RET_ERROR RCL_RET_ERROR
/** The operation exceeded its timeout. rcl's value is 2; ours is -2. */
#define RMW_RET_TIMEOUT RCL_RET_TIMEOUT
/** The operation is not supported by the active backend. rcl's is 3. */
#define RMW_RET_UNSUPPORTED RCL_RET_UNSUPPORTED
/** An argument to the function was invalid. rcl's value is 11; ours is -3. */
#define RMW_RET_INVALID_ARGUMENT RCL_RET_INVALID_ARGUMENT

/* Since phase-492 the `RCL_RET_*` codes are the generated header's own
 * constants (our values), so `RCL_RET_ERROR == RMW_RET_ERROR` holds here
 * exactly as it does upstream, with no second definition. */

/* ── Drift guards ─────────────────────────────────────────────────────────
 * Two mutations would break the mapping silently, and each has its own
 * assertion:
 *
 *   (a) someone renumbers `rcl_ret_t` — the first block pins every mapped
 *       code to the literal it has today, so a moved value fails HERE rather
 *       than at a consumer that stored the old one. Pinning six exact
 *       literals also makes the mapping provably INJECTIVE, so a ported
 *       `switch` over these labels keeps compiling (the probe proves that
 *       half, where a duplicate case label is a hard error).
 *
 *   (b) someone "fixes" the incompatibility by taking rcl's VALUES as well as
 *       its spelling — the second block asserts we did not. This is the
 *       mutation RFC-0089 names as forbidden, and without it the header would
 *       still compile after the change that breaks three FFI seams.
 */
#if defined(__cplusplus) && __cplusplus >= 201103L
#define NROS_RCL_COMPAT_ASSERT(cond, msg) static_assert(cond, msg)
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
#define NROS_RCL_COMPAT_ASSERT(cond, msg) _Static_assert(cond, msg)
#else
#define NROS_RCL_COMPAT_ASSERT(cond, msg)
#endif

/* (a) ours have not moved. */
NROS_RCL_COMPAT_ASSERT(RCL_RET_OK == 0, "RCL_RET_OK moved; rcl_compat.h's mapping is stale");
NROS_RCL_COMPAT_ASSERT(RCL_RET_ERROR == -1, "RCL_RET_ERROR moved; rcl_compat.h's mapping is stale");
NROS_RCL_COMPAT_ASSERT(RCL_RET_TIMEOUT == -2,
                       "RCL_RET_TIMEOUT moved; rcl_compat.h's mapping is stale");
NROS_RCL_COMPAT_ASSERT(RCL_RET_INVALID_ARGUMENT == -3,
                       "RCL_RET_INVALID_ARGUMENT moved; rcl_compat.h's mapping is stale");
NROS_RCL_COMPAT_ASSERT(RCL_RET_NOT_INIT == -7,
                       "RCL_RET_NOT_INIT moved; rcl_compat.h's mapping is stale");
NROS_RCL_COMPAT_ASSERT(RCL_RET_UNSUPPORTED == -16,
                       "RCL_RET_UNSUPPORTED moved; rcl_compat.h's mapping is stale");

/* (b) we took rcl's SPELLINGS and not its VALUES. RFC-0089: renumbering
 *     rcl_ret_t would silently flip every stored return code across the C,
 *     C++ and Rust FFI seams. OK is the sole code where the two agree. */
NROS_RCL_COMPAT_ASSERT(RCL_RET_OK == 0, "RCL_RET_OK must be 0 -- the one value ours and rcl share");
NROS_RCL_COMPAT_ASSERT(RCL_RET_ERROR != 1,
                       "rcl_ret_t was renumbered to rcl's values; see RFC-0089 -- MAP, never "
                       "renumber (three FFI seams store these)");
NROS_RCL_COMPAT_ASSERT(RCL_RET_TIMEOUT != 2,
                       "rcl_ret_t was renumbered to rcl's values; see RFC-0089");
NROS_RCL_COMPAT_ASSERT(RCL_RET_UNSUPPORTED != 3,
                       "rcl_ret_t was renumbered to rcl's values; see RFC-0089");
NROS_RCL_COMPAT_ASSERT(RCL_RET_INVALID_ARGUMENT != 11,
                       "rcl_ret_t was renumbered to rcl's values; see RFC-0089");
NROS_RCL_COMPAT_ASSERT(RCL_RET_NOT_INIT != 101,
                       "rcl_ret_t was renumbered to rcl's values; see RFC-0089");

/* The type alias must be able to hold what upstream's `int32_t` holds. */
NROS_RCL_COMPAT_ASSERT(sizeof(rcl_ret_t) == 4, "rcl_ret_t must be 32-bit, as upstream's rmw_ret_t");

/* ═══════════════════════════════════════════════════════════════════════════
 * 2. Handle types
 * ═══════════════════════════════════════════════════════════════════════════
 *
 * A ported line is `rcl_publisher_t pub = rcl_get_zero_initialized_publisher();`,
 * so the zero-init aliases below are unusable without these.
 *
 * The alias is on the NAME and the LIFECYCLE idiom (caller owns the struct by
 * value, zero-initialises it, passes it to `_init`), which is the same in both
 * APIs. The LAYOUT is nano-ros's: a ported file that reaches into `.impl` or
 * any other upstream member fails to compile, which is correct — RFC-0035 puts
 * the entity internals behind the API on purpose.
 *
 * C has no portable way to deprecate a typedef (see `rclc/visibility_control.h`), so
 * these carry a comment and no attribute, as the repo's convention requires.
 */
/* Since phase-492 the entity types carry these names themselves
 * (`rcl_node_t`, `rcl_publisher_t`, ..., `rclc_executor_t`, `rclc_support_t`
 * are the generated header's own structs), so no alias is needed here. */

/*
 * NOT aliased: `rosidl_message_type_support_t` / `rosidl_service_type_support_t`.
 *
 * The handle typedefs above are honest because those types are OPAQUE on both
 * sides -- no member is part of the contract, so aliasing the name claims only
 * the lifecycle idiom, and a ported file reaching into `.impl` fails to
 * compile (RFC-0035 puts entity internals behind the API on purpose).
 *
 * A rosidl typesupport is the opposite: its MEMBERS are the contract. Upstream
 * carries `typesupport_identifier`, `data` and the `func` dispatcher that
 * resolves a nested typesupport; ours is a flat descriptor of `type_name`,
 * `type_hash` and `serialized_size_max` (`rclc_message_type_t` in
 * `<rclc/detail/nros_generated.h>` — by NAME: that header is cbindgen output and the
 * line number this cited had moved on by 105 lines). Same
 * ROLE, different substance, so the name is not taken. The `init` forwarders
 * below therefore spell this parameter `const struct rclc_message_type_t *`,
 * which costs a ported call site nothing: the argument it passes comes from
 * OUR codegen either way, because `ROSIDL_GET_MSG_TYPE_SUPPORT` is absent and
 * a ported line that tries to produce one fails to compile.
 */

/* ═══════════════════════════════════════════════════════════════════════════
 * 3. What used to be here: TWELVE forwarders, now IDENTITY and therefore gone
 * ═══════════════════════════════════════════════════════════════════════════
 *
 * phase-417 stage 6 (2026-09-04) renamed the C entry points themselves, so
 * every function forwarder this header used to carry now has the SAME name on
 * both sides. An identity `static inline` is not a shim, it is infinite
 * recursion waiting for `-O0`, and the compiler says so first: a
 * `static inline rcl_get_zero_initialized_node` beside the non-static
 * declaration in `<rclc/detail/nros_generated.h>` is
 * `error: static declaration of 'rcl_get_zero_initialized_node' follows
 * non-static declaration`. They are deleted rather than argued away — RFC-0089
 * calls this "dissolving by construction", and it is the property that keeps a
 * compat layer from rotting into permanent debt.
 *
 * The twelve, and where each one now lives natively:
 *
 *   rcl_get_zero_initialized_node                  nros_generated.h (node.rs)
 *   rcl_get_zero_initialized_publisher             nros_generated.h (publisher.rs)
 *   rcl_get_zero_initialized_subscription          nros_generated.h (subscription.rs)
 *   rcl_get_zero_initialized_client                nros_generated.h (service.rs)
 *   rcl_get_zero_initialized_service               nros_generated.h (service.rs)
 *   rcl_get_zero_initialized_timer                 nros_generated.h (timer.rs)
 *   rcl_get_zero_initialized_guard_condition       nros_generated.h (guard_condition.rs)
 *   rclc_executor_get_zero_initialized_executor    nros_generated.h (executor.rs)
 *   rcl_trigger_guard_condition                    nros_generated.h (guard_condition.rs)
 *   rclc_publisher_init_default                    nros_generated.h (publisher.rs)
 *   rclc_client_init_default                       nros_generated.h (service.rs)
 *   rclc_node_init_default                         nros_generated.h (node.rs)  <-- REORDERED
 *
 * `rclc_node_init_default` is the one that moved arguments, and the rename and
 * the reorder landed TOGETHER on purpose. RFC-0089's corrected hazard note:
 *
 *     C   : warning: passing argument 1 of 'f' from incompatible pointer type
 *           ...even under -Wall -Wextra.
 *     C++ : error: cannot convert 'support_t*' to 'const char*'
 *
 * So a C reorder is silent-by-default for exactly the callers it must not be
 * silent for — out-of-tree consumers, who do not build with our flags. A
 * rename makes the stale call fail on the IDENTIFIER, which C does diagnose
 * fatally. Step B retired the old `nros_node_init` forwarder, so that is now
 * the ONLY diagnostic a stale call gets — an unknown identifier, not a
 * deprecation naming its replacement. The reorder is the reason the identifier
 * had to change at all: had the name been kept, a stale four-argument call
 * would still compile with a warning and build a node with its name in the
 * support slot.
 *
 * TWO of rclc's six `_init_default` constructors were refused here as recently
 * as stage 5 and are now native, because the reason was withdrawn rather than
 * worked around:
 *
 *   rclc_subscription_init_default(sub, node, type_support, topic_name)
 *   rclc_service_init_default(service, node, type_support, service_name)
 *
 * Ours used to carry `callback` and `context` in two extra positions, and the
 * ledger attributed that to RFC-0041 and to a rule called
 * `executor-owns-no-entity-storage`. RFC-0089 checked both: RFC-0041's
 * normative content is the DISPATCH MODEL and says nothing about a binding
 * site, and `executor-owns-no-entity-storage` has zero occurrences in
 * `docs/design/` — it is a phrase the ledger invented and then cited ten
 * times. The one real constraint (`c:timer_exchange_callback`) forbids
 * SWAPPING a callback on a live entity, not supplying one at registration.
 *
 * So the callback moved to where rclc puts it:
 *   - typed path: `rclc_executor_add_subscription_with_context(exec, sub,
 *     msg, cb, ctx, invocation)` — rclc's own, since phase-492 W8 put the
 *     codec in the descriptor;
 *   - byte path: `rclc_executor_add_subscription_raw(exec, sub, cb, ctx,
 *     invocation)` and `rclc_executor_add_service_raw(exec, service, cb, ctx)`.
 *     These keep the `nros_` prefix on purpose: rclc's `add_subscription` and
 *     `add_service` deliver DESERIALIZED messages into caller-owned storage,
 *     so a byte-oriented entry point must not wear their names.
 *
 * ── The typesupport parameter still keeps OUR type ────────────────────────
 *
 * `rclc_publisher_init_default` and friends spell their third parameter
 * `const struct rclc_message_type_t *` (or `rclc_service_type_t`), not
 * `rosidl_message_type_support_t`. Settled 2026-09-04, narrowed 2026-10-12
 * (phase-492 W8): `ROSIDL_GET_MSG_TYPE_SUPPORT(pkg, msg, Name)` is adopted and
 * yields our descriptor, so a ported init line compiles unchanged; the STRUCT
 * stays undeclared, because a rosidl typesupport's MEMBERS are its contract —
 * the `typesupport_identifier`, the `data`, and the `func` dispatcher that
 * resolves the implementation at runtime — against our `type_name` /
 * `type_hash` / `serialized_size_max` / `serialize` / `deserialize`. Adopting the name would claim
 * a dispatcher we do not have, and the "compiles and differs" that follows is exactly what RFC-0089
 * forbids. It costs a ported call site nothing, because the argument comes from OUR codegen either
 * way: `ROSIDL_GET_MSG_TYPE_SUPPORT` does not exist here, so a ported line that tries to produce
 * one fails to compile.
 *
 * ═══════════════════════════════════════════════════════════════════════════
 * 4. ADOPTED by phase-515 C1: `rclc_timer_init_default` and `rcl_timer_init`
 * ═══════════════════════════════════════════════════════════════════════════
 *
 * The last `_init_default` to converge, and the reason it took longest was the
 * CALLBACK CONTRACT rather than the arity:
 *
 *   rcl_timer_callback_t = void (*)(rcl_timer_t *, int64_t last_call_time)
 *
 * Until phase-515 ours was `void (*)(rcl_timer_t *, void *context)`: two
 * parameters, both leading with the timer, and the SECOND carrying entirely
 * different information. A ported timer callback was a different FUNCTION, so
 * the init took a distinct name (`rclc_timer_init_with_context`) rather than
 * compile-and-differ under rclc's.
 *
 * phase-515 decision D2 took rclc's contract and removed ours. The context
 * was always recoverable: every in-tree caller (34) embedded the timer in the
 * struct its context pointed at, or in a struct beside it, and the executor
 * hands the callback the caller's OWN timer, never a copy. So the state is
 * reached the standard C way, `RCLC_CONTAINER_OF(timer, my_t, timer)` in
 * `<rclc/timer.h>`, which is the `offsetof` arithmetic an rclc program writes
 * by hand. Now native, with upstream's signatures:
 *
 *   rclc_timer_init_default(timer, support, timeout_ns, callback)
 *   rclc_timer_init_default2(timer, support, timeout_ns, callback, autostart)   (Jazzy)
 *   rcl_timer_init(timer, clock, context, period, callback, allocator)
 *   rcl_timer_init2(timer, clock, context, period, callback, allocator, autostart) (Jazzy)
 *
 * The envelope, stated at each definition:
 *   - `support` and `context` may be NULL, which rclc and rcl refuse: neither
 *     is read, and a C component the generated entry hosts has no support.
 *   - `allocator` must be the default; a caller-supplied one is refused.
 *   - `last_call_time` is the executor's elapsed count at dispatch, the value
 *     `rcl_timer_get_time_since_last_call` reports: microsecond resolution
 *     (issue #505), measured from the previous due point.
 *
 * The clock goes where rcl puts it, SECOND (phase-430 W1 took that position
 * under the name `rclc_timer_init_on_clock` while the contract still
 * differed). Jazzy's call info (`rcl_timer_call_info_t`) reaches a callback
 * registered with `rclc_timer_init_with_info(timer, clock, period, cb,
 * autostart)` rather than through `rcl_timer_call_with_info`, a dispatch call
 * our executor makes for the user; that callback carries no context either.
 *
 * ── Also still refused, and why, so the absence is a record ───────────────
 *
 * * ADOPTED by phase-492 W1.b, so no longer in this list: the `rcl_*_fini`
 *   of publisher / subscription / client / service / action client / action
 *   server take rcl's `(entity, node)` (the node is checked non-NULL; teardown
 *   needs nothing from it); `rcl_node_get_domain_id` writes rcl's `size_t *`;
 *   `rcl_timer_get_time_since_last_call` writes rcl's `int64_t *`;
 *   `rcl_clock_get_now` writes rcl's `int64_t` nanosecond count (the
 *   `struct rclc_time_t` form is `rclc_clock_get_now_time`); `rcl_clock_init`,
 *   `rclc_support_init` and `rclc_executor_init` take rcl's arguments,
 *   including `rcl_allocator_t` (accepted, never used to allocate — a
 *   caller-supplied one is refused) and `&support.context`; and
 *   `rcl_action_client_init` takes rcl's `(client, node, type_support, name,
 *   options)`.
 * * `rcl_timer_get_period` — upstream returns the value through an `int64_t *`
 *   out-parameter; ours returns it. Different data flow, not a different word.
 * * `rcl_timer_get_time_until_next_call` — was in the bullet above and is NOT
 *   any more. phase-417 stage 3 gave ours upstream's shape exactly,
 *   `rcl_ret_t (const rcl_timer_t *, int64_t *)`: the caller-supplied "now"
 *   is gone, the value moved to a SIGNED out-parameter so overdue is
 *   expressible, and the status return carries the error channel the old
 *   `uint64_t` had spent on `0`. What remains between it and the NAME is the
 *   entry-point rename, which is stage 6's step and not a contract difference.
 *   Two envelopes a ported caller should know, both stated on the function:
 *   the arena's timer accounting is microsecond-based (issue #505), so the
 *   nanosecond value is a microsecond quantity scaled by 1000; and
 *   `RCL_RET_TIMER_INVALID` / `RCL_RET_TIMER_CANCELED` have no counterpart
 *   here (see the constant block), so those and "not registered with an
 *   executor" all arrive as `RCL_RET_NOT_INIT`.
 *   **The NAME was taken in W5.c (2026-09-13).** This bullet said the rename
 *   "is stage 6's step"; stage 6 renamed twelve entry points and left this one,
 *   and nothing noticed until W5.c added its absolute sibling
 *   `rcl_timer_get_next_call_time` beside it — at which point one family had
 *   two prefixes with a stated reason for only one of them
 *   (`rcl_timer_get_time_since_last_call` kept its old prefix then because its
 *   out-parameter was `uint64_t *` against upstream's `int64_t *`; phase-492
 *   W1.b gave it rcl's type). It is
 *   `rcl_timer_get_time_until_next_call` now, with no forwarder, per stage 6
 *   step B's rule that a stale call should fail on the IDENTIFIER.
 * * `rcl_guard_condition_init`, `rcl_node_get_fully_qualified_name`, the
 *   `rclc_lifecycle_*` family, `rcl_node_resolve_name` (ours,
 *   `rclc_node_resolve_name`, writes a caller buffer where rcl allocates) and
 *   `rcl_action_server_init` (ours, `rclc_action_server_init_with_callbacks`,
 *   binds the callbacks rclc binds at `rclc_executor_add_action_server`) —
 *   every one differs in ARITY or data flow, and keeps a distinct name.
 * * `rcl_take_request` / `rcl_take_response` — upstream hands back a
 *   DESERIALIZED message plus an `rmw_request_id_t` out-struct
 *   (`rcl/service.h:279`, `rcl/client.h:301`); ours are the CDR-byte drains
 *   `rclc_polling_service_take_request_raw(service, buf, buf_len, int64_t *seq)` and
 *   `rclc_client_take_response(client, buf, buf_len, size_t *out_len)`.
 *   Different arity, different argument order, different data. This is
 *   `PollingSubscription::take()`'s class exactly — a plausible name over an
 *   opposite data contract — and it is the reason RFC-0089 exists.
 * * `RCL_RET_BAD_ALLOC`, `RCL_RET_ALREADY_INIT`, `RCL_RET_*_INVALID`,
 *   `RCL_RET_*_TAKE_FAILED`, `RCL_RET_TIMER_CANCELED`, the wait-set codes —
 *   see the note above the constant block.
 */

/* ═══════════════════════════════════════════════════════════════════════════
 * 6. The executor's entity registration: one adoption, two refusals
 * ═══════════════════════════════════════════════════════════════════════════
 *
 * CORRECTED 2026-09-18 (phase-417 W5.e). This section used to tell a reader
 * that the typed subscription entry point did not exist and that "the honest
 * move is to name the shape a faithful alias needs and wait for it". It had
 * SHIPPED — on 2026-09-04, in the very commit that wrote the sentence (W5.a and
 * W5.b landed together) — and the name sketched here as the thing to wait for,
 * `nros_subscription_typed_callback_t`, is the shipped one with two words
 * swapped. A reader following this section would have concluded a capability
 * was missing and written their own CDR, which is the failure the whole
 * campaign exists to remove. Everything below is read off the header rather
 * than predicted from it.
 *
 * Citations here name IDENTIFIERS, never line numbers. `<rclc/detail/nros_generated.h>`
 * is cbindgen output, so every regeneration moves it: all four line numbers
 * this section used to carry pointed at unrelated code by the time anyone
 * looked (the same rot W5.d found one section up).
 *
 * ── The typed subscription callback: ADOPTED ─────────────────────────
 *
 *   rclc: typedef void (*rclc_subscription_callback_with_context_t)(
 *             const void *msg, void *context);
 *
 * Same two parameters, same order, same meanings. It was our
 * `rclc_typed_subscription_callback_t` plus an alias here; phase-492 W1.b gave
 * the definition rclc's name, and per RFC-0089 rule 1 the twin is deleted, not
 * aliased. A nano-ros program names it without a compat header.
 */

/*
 * ── The context-LESS `rclc_executor_add_subscription`: ADOPTED (phase-515 C2) ─
 *
 *   rclc: rcl_ret_t rclc_executor_add_subscription(rclc_executor_t *,
 *             rcl_subscription_t *, void *msg,
 *             rclc_subscription_callback_t callback,
 *             rclc_executor_handle_invocation_t invocation)
 *         typedef void (*rclc_subscription_callback_t)(const void *msgin);
 *
 * It was refused on the argument that adapting a `void (*)(const void *)` to
 * the context-carrying dispatch needs a trampoline that REMEMBERS the original
 * pointer, i.e. state in the wrapper (RFC-0019/0020). The pointer does not need
 * the wrapper: the executor's arena entry already owns a `context` slot, and
 * the context-less form registers a fixed trampoline with the user's callback
 * IN that slot. The trampoline only translates the ABI, which is what
 * RFC-0020's checklist asks of one.
 *
 * ── `rclc_executor_add_subscription_with_context`: ADOPTED (phase-492 W8) ─
 *
 *   rclc: rcl_ret_t rclc_executor_add_subscription_with_context(
 *             rclc_executor_t *executor, rcl_subscription_t *subscription,
 *             void *msg, rclc_subscription_callback_with_context_t callback,
 *             void *context, rclc_executor_handle_invocation_t invocation)
 *
 * Ours is the same six arguments. It was refused here by ARITY while
 * `rclc_message_type_t` had no codec: the deserialiser had to arrive as a
 * seventh argument, and a generated per-type macro collapsed it away. The
 * descriptor now carries `serialize` / `deserialize` and the subscription keeps
 * the descriptor it was created with, so the registration reads the
 * deserialiser from there and the macro is gone.
 *
 * ── `rclc_executor_add_service{,_with_context}`: REFUSED, by DATA ───────
 *
 *   rclc: rcl_ret_t rclc_executor_add_service_with_context(
 *             rclc_executor_t *, rcl_service_t *, void *request_msg,
 *             void *response_msg, rclc_service_callback_with_context_t,
 *             void *context)
 *         typedef void (*rclc_service_callback_with_context_t)(
 *             const void *request, void *response, void *context);
 *   ours: rcl_ret_t rclc_executor_add_service_raw(
 *             struct rclc_executor_t *, struct rcl_service_t *,
 *             rclc_service_raw_callback_t callback, void *context)
 *         typedef bool (*rclc_service_raw_callback_t)(
 *             const uint8_t *request, size_t request_len, uint8_t *response,
 *             size_t response_capacity, size_t *response_len, void *context);
 *
 * Two things this paragraph used to get wrong. Ours is NOT "bound at creation"
 * any more — phase-417 stage 6 moved the callback to REGISTRATION, where rclc
 * puts it, and `rclc_executor_add_service_raw` is that call. And the typed
 * service path is no longer missing: phase-417 W5.e ships it per service, out
 * of `packs/c/service.h.jinja` —
 *
 *     void cb(const <Srv>_Request *request, <Srv>_Response *response,
 *             void *context);
 *
 *     <Srv>__service_handler_init(&handler, cb, &ctx);
 *     <Srv>__service_init(&service, &node, "/name");
 *     <Srv>__executor_add_service(&executor, &service, &handler);
 *
 * — the CALLER owning both payload structs, which is exactly what rclc's
 * `request_msg` / `response_msg` arguments are for, and the callback carrying
 * the `void *context` rclc puts behind its separate `_with_context`
 * registration. The client half is the same shape one entity over
 * (`<Srv>__client_handler_t`, `<Srv>__client_set_response_callback`).
 *
 * The REGISTRATION still cannot be aliased, and for a reason the subscription's
 * does not share. Ours carries ONE context word, so it cannot take rclc's
 * `request_msg` and `response_msg` as two separate caller arguments: both
 * payload structs travel together inside the generated
 * `<Srv>__service_handler_t`, which is also where the generated trampoline finds
 * them. Collapsing two caller arguments into one block is not something a name
 * can hide. And ours RETURNS a value where rclc's callback returns `void` —
 * that return is what decides whether a malformed request puts a reply on the
 * wire at all, so the two callbacks do not merely differ in payload shape.
 *
 * Deliberately not aliased onto anything today. Two arenas for one concept is
 * the drift RFC-0019 exists to prevent.
 */

#endif /* collision guard */

#endif /* NROS_RCL_COMPAT_H */
