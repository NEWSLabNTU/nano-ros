/*
 * Phase 379 W5 — the NEGATIVE half of `service_name_aliases.c`.
 *
 * That file proves the old `send_reply` / un-suffixed `send_response` spellings
 * still COMPILE. This one proves they still WARN. A deprecation nobody is told
 * about is just an alias, and the reason this family used NROS_DEPRECATED_MSG
 * forwarders was to get the diagnostic — so "the attribute reaches callers" is
 * the thing worth pinning.
 *
 * `just check c` compiles this with `-Werror=deprecated-declarations` and
 * requires it to FAIL. It is a normal, valid TU otherwise; only that flag turns
 * the warning into an error. Written as an expected failure because a clean
 * compile is exactly what a silently-dropped attribute looks like.
 *
 * Reconstructed 2026-08-27 alongside `service_name_aliases.c`; see the note
 * there.
 */

#include "nros/service.h"

nros_ret_t nros_service_deprecation_probe(struct nros_service_t* service);
nros_ret_t nros_service_deprecation_probe(struct nros_service_t* service) {
    return nros_service_send_reply_raw(service, 0, (const uint8_t*) "", 0);
}
