/*
 * Phase 379 W5 — the NEGATIVE half of `param_name_aliases.c`.
 *
 * That file proves the old `nros_param_*` spellings still COMPILE. This one
 * proves they still WARN. A deprecation nobody is told about is just an alias,
 * and the reason this family used NROS_DEPRECATED_MSG forwarders instead of
 * issue 0338's plain macros was to get the diagnostic — so "the attribute
 * reaches callers" is the thing worth pinning, not an implementation detail.
 *
 * `just check c` compiles this with `-Werror=deprecated-declarations` and
 * requires it to FAIL. It is a normal, valid TU otherwise; only that flag turns
 * the warning into an error. Written as an expected failure because a clean
 * compile is exactly what a silently-dropped attribute looks like.
 */

#include "nros/parameter.h"

nros_ret_t nros_param_deprecation_probe(nros_param_server_t* server);
nros_ret_t nros_param_deprecation_probe(nros_param_server_t* server) {
    return nros_param_declare_bool(server, "x", true);
}
