/* issue 1609 — a C action server / client can be REMOVED from its executor,
 * and a create / remove loop runs forever.
 *
 * Before 1609 the C API had no executor-remove call for any entity, and
 * `nros_action_server_fini` / `nros_action_client_fini` reset only the C
 * struct: the arena entry `nros_executor_add_action_*` registered stayed live
 * with its callback `context` naming that struct, the action's RMW entities
 * stayed advertised, and `handle_count` never came back down. Issue 1496 gave
 * the executor arena its release path and the C++ tier used it from its
 * destructors; this is the C twin, through
 * `nros_executor_remove_action_{server,client}`.
 *
 * Every shape of the defect compiles — `fini` returning NROS_RET_OK while
 * leaving the entry is the defect — so only a run can see it. Three numbers,
 * each read from a different owner, because each half of the release is
 * invisible from the other two:
 *
 *   - `nros_executor_get_handle_count` — the C executor's own table;
 *   - `nros_executor_get_arena_used`   — the Rust arena's high-water mark. It
 *     must equal the FIRST cycle's on every later cycle: that is the
 *     measurement that the released bytes are REUSED, not merely not yet
 *     exhausted;
 *   - `nros_stub_rmw_live_entities`    — the backend's count of entities it
 *     created and was not asked to destroy. It must return to its baseline
 *     after every remove: that is the action leaving the graph.
 *
 * And a negative control, in the same binary: the PRE-1609 teardown (`fini`
 * alone) on the same loop must exhaust the executor, otherwise the positive
 * loop could pass on an executor that never ran out in the first place.
 *
 * Same stub backend and same archive as the other run probes in this lane, so
 * it needs no router, no agent and no network.
 */

#include "stub_rmw_backend.h"

#include <nros/nros.h>

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

static int s_failures = 0;

#define CHECK(cond, msg)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            fprintf(stderr, "FAIL: %s (%s:%d)\n", (msg), __FILE__, __LINE__);                      \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

#define CHECK_RET(expr, expected, msg)                                                             \
    do {                                                                                           \
        nros_ret_t _r = (expr);                                                                    \
        if (_r != (expected)) {                                                                    \
            fprintf(stderr, "FAIL: %s -- got %d, expected %d (%s:%d)\n", (msg), (int)_r,           \
                    (int)(expected), __FILE__, __LINE__);                                          \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

/* More cycles than the executor has handles (max_handles below) or callback
 * slots (NROS_EXECUTOR_MAX_CBS), and — asserted, not assumed — more than its
 * arena holds of these entries. */
#define CYCLES 200
#define MAX_HANDLES 4

static const nros_action_type_t ACTION_TYPE = {
    .type_name = "example_interfaces::action::dds_::Fibonacci_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .goal_serialized_size_max = 64,
    .result_serialized_size_max = 64,
    .feedback_serialized_size_max = 64,
};

static enum nros_goal_response_t reject_goal(struct nros_action_server_t* server,
                                             const struct nros_goal_handle_t* goal,
                                             const uint8_t* request, size_t len, void* ctx) {
    (void)server;
    (void)goal;
    (void)request;
    (void)len;
    (void)ctx;
    return NROS_GOAL_REJECT;
}

typedef struct {
    struct nros_executor_t executor;
    struct nros_node_t node;
} rig_t;

static void rig_open(rig_t* rig, struct nros_support_t* support, const char* node_name) {
    rig->executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(nros_executor_init(&rig->executor, support, MAX_HANDLES), NROS_RET_OK,
              "executor initialised");
    nros_node_options_t opts = rcl_node_get_default_options();
    rig->node = rcl_get_zero_initialized_node();
    CHECK_RET(nros_executor_node_init(&rig->executor, &rig->node, node_name, &opts), NROS_RET_OK,
              "node bound to the executor");
}

static void rig_close(rig_t* rig) {
    (void)rcl_node_fini(&rig->node);
    CHECK_RET(rclc_executor_fini(&rig->executor), NROS_RET_OK, "executor finalised");
}

/* ---- 1. server: add / remove / fini, CYCLES times ----------------------- */

static void server_cycles(struct nros_support_t* support) {
    rig_t rig;
    rig_open(&rig, support, "server_cycles");

    const size_t capacity = nros_executor_get_arena_capacity(&rig.executor);
    const size_t base_used = nros_executor_get_arena_used(&rig.executor);
    const int32_t base_live = nros_stub_rmw_live_entities();
    CHECK(capacity > 0, "the arena reports a capacity");

    size_t high_water = 0;
    int completed = 0;
    for (int i = 0; i < CYCLES; i++) {
        struct nros_action_server_t server = rcl_action_get_zero_initialized_server();
        if (nros_action_server_init(&server, &rig.node, "fibonacci", &ACTION_TYPE, reject_goal,
                                    NULL, NULL, NULL) != NROS_RET_OK) {
            fprintf(stderr, "FAIL: server cycle %d: init\n", i);
            s_failures++;
            break;
        }
        nros_ret_t r = nros_executor_add_action_server(&rig.executor, &server);
        if (r != NROS_RET_OK) {
            fprintf(stderr, "FAIL: server cycle %d: add returned %d\n", i, (int)r);
            s_failures++;
            break;
        }
        if (i == 0) {
            high_water = nros_executor_get_arena_used(&rig.executor);
            CHECK(high_water > base_used, "the first registration claimed arena bytes");
            CHECK(nros_stub_rmw_live_entities() - base_live == 5,
                  "an action server is five entities: three service servers + two publishers");
        } else if (nros_executor_get_arena_used(&rig.executor) != high_water) {
            fprintf(stderr,
                    "FAIL: server cycle %d: arena used %zu, first cycle %zu -- the released "
                    "region was not reused\n",
                    i, nros_executor_get_arena_used(&rig.executor), high_water);
            s_failures++;
            break;
        }
        CHECK(nros_executor_get_handle_count(&rig.executor) == 1, "one handle while registered");

        CHECK_RET(nros_executor_remove_action_server(&rig.executor, &server), NROS_RET_OK,
                  "remove");
        if (nros_executor_get_handle_count(&rig.executor) != 0 ||
            nros_stub_rmw_live_entities() != base_live) {
            fprintf(stderr,
                    "FAIL: server cycle %d: after remove handle_count=%d live=%d (base %d)\n", i,
                    nros_executor_get_handle_count(&rig.executor), nros_stub_rmw_live_entities(),
                    base_live);
            s_failures++;
            break;
        }
        if (i == 0) {
            /* A second remove has nothing to remove — and must not release
             * whatever registers into that slot next. */
            CHECK_RET(nros_executor_remove_action_server(&rig.executor, &server),
                      NROS_RET_NOT_FOUND, "a second remove finds nothing");
        }
        CHECK_RET(nros_action_server_fini(&server), NROS_RET_OK, "fini after remove");
        completed++;
    }
    CHECK(completed == CYCLES, "every server cycle completed");
    CHECK(CYCLES > MAX_HANDLES && (size_t)CYCLES * (high_water - base_used) > capacity,
          "precondition: the loop out-runs both the handle table and the arena");
    printf("  server: %d cycles, arena used pinned at %zu of %zu (entry %zu bytes), "
           "live entities back to %d\n",
           completed, high_water, capacity, high_water - base_used, base_live);
    rig_close(&rig);
}

/* ---- 2. client: the same, for the client half --------------------------- */

static void client_cycles(struct nros_support_t* support) {
    rig_t rig;
    rig_open(&rig, support, "client_cycles");

    const size_t capacity = nros_executor_get_arena_capacity(&rig.executor);
    const size_t base_used = nros_executor_get_arena_used(&rig.executor);
    const int32_t base_live = nros_stub_rmw_live_entities();

    size_t high_water = 0;
    int completed = 0;
    for (int i = 0; i < CYCLES; i++) {
        struct nros_action_client_t client = rcl_action_get_zero_initialized_client();
        if (nros_action_client_init(&client, &rig.node, "fibonacci", &ACTION_TYPE) != NROS_RET_OK) {
            fprintf(stderr, "FAIL: client cycle %d: init\n", i);
            s_failures++;
            break;
        }
        nros_ret_t r = nros_executor_add_action_client(&rig.executor, &client);
        if (r != NROS_RET_OK) {
            fprintf(stderr, "FAIL: client cycle %d: add returned %d\n", i, (int)r);
            s_failures++;
            break;
        }
        if (i == 0) {
            high_water = nros_executor_get_arena_used(&rig.executor);
            CHECK(high_water > base_used, "the first registration claimed arena bytes");
            CHECK(nros_stub_rmw_live_entities() > base_live,
                  "the client's entities reached the backend");
        } else if (nros_executor_get_arena_used(&rig.executor) != high_water) {
            fprintf(stderr,
                    "FAIL: client cycle %d: arena used %zu, first cycle %zu -- the released "
                    "region was not reused\n",
                    i, nros_executor_get_arena_used(&rig.executor), high_water);
            s_failures++;
            break;
        }
        CHECK_RET(nros_executor_remove_action_client(&rig.executor, &client), NROS_RET_OK,
                  "remove");
        if (nros_executor_get_handle_count(&rig.executor) != 0 ||
            nros_stub_rmw_live_entities() != base_live) {
            fprintf(stderr,
                    "FAIL: client cycle %d: after remove handle_count=%d live=%d (base %d)\n", i,
                    nros_executor_get_handle_count(&rig.executor), nros_stub_rmw_live_entities(),
                    base_live);
            s_failures++;
            break;
        }
        CHECK_RET(nros_action_client_fini(&client), NROS_RET_OK, "fini after remove");
        completed++;
    }
    CHECK(completed == CYCLES, "every client cycle completed");
    CHECK((size_t)CYCLES * (high_water - base_used) > capacity,
          "precondition: the loop out-runs the arena");
    printf("  client: %d cycles, arena used pinned at %zu of %zu (entry %zu bytes)\n", completed,
           high_water, capacity, high_water - base_used);
    rig_close(&rig);
}

/* ---- 3. negative control: the pre-1609 teardown exhausts ---------------- */

static void fini_alone_exhausts(struct nros_support_t* support) {
    rig_t rig;
    rig_open(&rig, support, "fini_alone");
    const int32_t base_live = nros_stub_rmw_live_entities();

    /* Storage outlives every iteration on purpose: the entries a fini-only
     * teardown leaves behind keep naming it, and this control must not turn
     * into a use-after-free of its own. */
    static struct nros_action_server_t servers[CYCLES];
    int failed_at = -1;
    nros_ret_t failed_with = NROS_RET_OK;
    for (int i = 0; i < CYCLES; i++) {
        servers[i] = rcl_action_get_zero_initialized_server();
        CHECK_RET(nros_action_server_init(&servers[i], &rig.node, "fibonacci", &ACTION_TYPE,
                                          reject_goal, NULL, NULL, NULL),
                  NROS_RET_OK, "control: init");
        nros_ret_t r = nros_executor_add_action_server(&rig.executor, &servers[i]);
        if (r != NROS_RET_OK) {
            failed_at = i;
            failed_with = r;
            (void)nros_action_server_fini(&servers[i]);
            break;
        }
        /* fini WITHOUT remove — the teardown every C image used before 1609. */
        CHECK_RET(nros_action_server_fini(&servers[i]), NROS_RET_OK, "control: fini");
    }
    CHECK(failed_at >= 0, "an executor that never runs out proves nothing: fini alone must "
                          "exhaust it, or the positive loop measured nothing");
    CHECK(failed_at <= MAX_HANDLES, "and it runs out at the handle table");
    CHECK(nros_stub_rmw_live_entities() - base_live == 5 * failed_at,
          "every fini'd-but-not-removed server is still on the graph");
    printf("  control: fini alone failed add #%d with %d; %d entities still live\n", failed_at,
           (int)failed_with, nros_stub_rmw_live_entities() - base_live);
    rig_close(&rig);
    CHECK(nros_stub_rmw_live_entities() == base_live,
          "rclc_executor_fini drops every entry it still held");
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct nros_support_t support = nros_support_get_zero_initialized();
    CHECK_RET(nros_support_init_rmw(&support, "stub://none", 45, "action_remove_cycles",
                                    NROS_STUB_RMW_NAME),
              NROS_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) {
        return 1;
    }

    server_cycles(&support);
    client_cycles(&support);
    fini_alone_exhausts(&support);

    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "action_remove_cycles: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("action_remove_cycles: OK\n");
    return 0;
}
