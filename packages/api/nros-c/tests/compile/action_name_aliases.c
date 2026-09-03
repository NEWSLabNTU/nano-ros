/*
 * Phase 379 W6 decision 3 — the active-goal count pair was merged into ONE
 * function in rcl's shape:
 *
 *     nros_ret_t nros_action_server_get_active_goal_count(server, size_t *out);
 *
 * This TU pins both halves of that change, the way `param_name_aliases.c` and
 * `service_name_aliases.c` pin theirs:
 *
 *   1. the merged entry point exists with the two-channel signature the header
 *      documents (return code = did the query work, out-param = the answer),
 *      and
 *   2. the OLD polling-tier spelling `nros_action_server_active_goal_count_raw`
 *      still compiles, with its ORIGINAL `int32_t` return convention, so a
 *      consumer gets a release to migrate.
 *
 * There is deliberately no third clause for the old CALLBACK-tier spelling.
 * That one was `size_t nros_action_server_get_active_goal_count(const
 * server)` — the same identifier the merged function now uses — and C has one
 * declaration per identifier, so no forwarder can exist beside it. Its old
 * callers get a compile error, which is the correct outcome for a signature
 * change and is why the merge was only affordable with zero callers in tree.
 * Pinning the merged signature in clause 1 IS the assertion that the old form
 * is gone.
 *
 * Compile-only (no main): the assertion is that these names resolve with these
 * types. Taking a function POINTER forces a real lookup and a real signature
 * match — a forwarder whose argument list drifted from the function it
 * forwards to fails HERE rather than at some consumer's call site.
 *
 * The old half is a `static inline` forwarder carrying NROS_DEPRECATED_MSG, so
 * naming it is supposed to warn. That warning is the point and is asserted
 * separately, by `action_deprecation_probe.c` under
 * `-Werror=deprecated-declarations`. Here it would be noise on a passing gate,
 * so it is suppressed for that clause only.
 */

#include "nros/action.h"

/* 1. The merged entry point, in rcl's two-channel shape. */
static nros_ret_t (*const k_get_active_goal_count)(struct nros_action_server_t*, size_t*) =
    nros_action_server_get_active_goal_count;

/* 2. The deprecated polling-tier spelling still resolves, still `int32_t`. */
#ifndef NROS_NO_DEPRECATED_ACTION_ALIASES
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"

static int32_t (*const k_active_goal_count_raw)(struct nros_action_server_t*) =
    nros_action_server_active_goal_count_raw;

#pragma GCC diagnostic pop
#endif /* NROS_NO_DEPRECATED_ACTION_ALIASES */

/* Reference them so no compiler prunes the lookups this file exists to force. */
const void* nros_action_name_alias_anchors[] = {
    (const void*)&k_get_active_goal_count,
#ifndef NROS_NO_DEPRECATED_ACTION_ALIASES
    (const void*)&k_active_goal_count_raw,
#endif
};
