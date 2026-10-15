/*
 * Issue 0338 — the executor's entity-registration family is spelled
 * `nros_executor_add_*`, matching rclc. It used to be `register_*` for every
 * kind EXCEPT `add_client`, so the family was not internally consistent and a C
 * user could not guess the verb.
 *
 * This TU pins BOTH halves of that change:
 *   1. the new `add_*` spelling exists for every kind, and
 *   2. the deprecated `register_*` aliases still compile, so a consumer gets one
 *      release to migrate.
 *
 * The guard-condition pair left this probe in phase-417 W4.e: the node is the
 * owner and `rclc_node_create_guard_condition` is the one creation shape, so
 * `nros_executor_add_guard_condition` and its `register_*` alias are retired
 * rather than migrated. A retired name has nothing to pin here.
 *
 * Compile-only (no main): the assertion is that these names resolve with these
 * signatures. Taking a function pointer forces a real lookup — a macro alias
 * that pointed at a nonexistent symbol would fail here.
 */

#include "rclc/executor.h"

/* 1. The rclc-spelled verbs exist. */
static rcl_ret_t (*const kAddTimer)(struct rclc_executor_t*,
                                    struct rcl_timer_t*) = rclc_executor_add_timer;
/* phase-487 W2: the service add takes its callback, as rclc's does. */
static rcl_ret_t (*const kAddService)(struct rclc_executor_t*, struct rcl_service_t*,
                                      rclc_service_raw_callback_t,
                                      void*) = rclc_executor_add_service_raw;
static rcl_ret_t (*const kAddClient)(struct rclc_executor_t*,
                                     struct rcl_client_t*) = rclc_executor_attach_client;

/* 2. The deprecated aliases still resolve, to the SAME functions. */
static rcl_ret_t (*const kOldTimer)(struct rclc_executor_t*,
                                    struct rcl_timer_t*) = rclc_executor_register_timer;
/* `rclc_executor_register_timer` is the one member of this family whose target
 * moved. Phase-417 stage 6 made `nros_executor_add_timer` a deprecated
 * forwarder onto `rclc_executor_add_timer`, so for one release the macro
 * resolved through TWO deprecations; step B retired that forwarder and the
 * macro now names `rclc_executor_add_timer` directly. It therefore no longer
 * warns on the way through — the 0338 aliases have their own retirement, and
 * until it lands this spelling is silent. Every other target below is a live
 * exported symbol. The assertion here is that the name still RESOLVES with
 * this signature. */

/* Silence "defined but not used" without needing a main(). */
const void* nros_executor_verb_alias_probe(void);
const void* nros_executor_verb_alias_probe(void) {
    (void)kAddTimer;
    (void)kAddService;
    (void)kAddClient;
    (void)kOldTimer;
    /* The alias must name the same entity as the new spelling.
     * `nros_executor_register_service` went with its two-argument target in
     * phase-487 W2, so the timer is the pair left to compare. */
    return (kOldTimer == kAddTimer) ? (const void*)kAddTimer : (const void*)0;
}
