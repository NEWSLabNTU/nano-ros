/*
 * Phase 379 W6 decision 3 — the NEGATIVE half of `action_name_aliases.c`.
 *
 * That file proves the old `nros_action_server_active_goal_count_raw` spelling
 * still COMPILES. This one proves it still WARNS. A deprecation nobody is told
 * about is just an alias, and the reason this rename used an
 * NROS_DEPRECATED_MSG forwarder rather than a macro was to get the diagnostic —
 * so "the attribute reaches callers" is the thing worth pinning.
 *
 * `just check c` compiles this with `-Werror=deprecated-declarations` and
 * requires it to FAIL. It is a normal, valid TU otherwise; only that flag turns
 * the warning into an error. Written as an expected failure because a clean
 * compile is exactly what a silently-dropped attribute looks like.
 */

#include "nros/action.h"

int32_t nros_action_deprecation_probe(struct nros_action_server_t* server);
int32_t nros_action_deprecation_probe(struct nros_action_server_t* server) {
    return nros_action_server_active_goal_count_raw(server);
}
