/* issue 1631 — a C subscription, timer, service or client can be REMOVED from
 * its executor, and a create / remove loop of each runs forever.
 *
 * `action_remove_cycles.c` (issue 1609) is the action half; this is the other
 * four kinds, in the same shape and with the same three owners read:
 *
 *   - `nros_executor_get_handle_count` — the C executor's own table;
 *   - `nros_executor_get_arena_used`   — the Rust arena's high-water mark. It
 *     must equal the FIRST cycle's on every later cycle: the released bytes
 *     are REUSED, not merely not yet exhausted. The loop runs until it has
 *     claimed at least twice the arena's capacity, so "not yet exhausted"
 *     cannot pass it;
 *   - `nros_stub_rmw_live_entities`    — the backend's count of entities it
 *     created and was not asked to destroy. It must return to its baseline
 *     after every remove (a timer never reaches the backend, so its delta is
 *     zero — asserted, not skipped).
 *
 * Plus, per kind: a second remove answers NOT_FOUND (a stale handle must not
 * release whatever registers into the slot next), and a NEGATIVE CONTROL — the
 * pre-1631 teardown, `fini` alone, on the same loop must exhaust the executor
 * at its handle table. Without it the positive loop could pass on an executor
 * that never runs out.
 *
 * Same stub backend as the other run probes in `just check c`: no router, no
 * agent, no network.
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

#define MIN_CYCLES 200
#define MAX_CYCLES 200000
#define MAX_HANDLES 4
#define CONTROL_SLOTS 16

static const nros_message_type_t MSG_TYPE = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 64,
};

static const nros_service_type_t SRV_TYPE = {
    .type_name = "example_interfaces::srv::dds_::AddTwoInts_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
};

static void on_message(const uint8_t* data, size_t len, void* ctx) {
    (void)data;
    (void)len;
    (void)ctx;
}

static void on_timer(struct nros_timer_t* timer, void* ctx) {
    (void)timer;
    (void)ctx;
}

static bool on_request(const uint8_t* req, size_t req_len, uint8_t* resp, size_t resp_cap,
                       size_t* resp_len, void* ctx) {
    (void)req;
    (void)req_len;
    (void)resp;
    (void)resp_cap;
    (void)resp_len;
    (void)ctx;
    return false;
}

typedef struct {
    struct nros_executor_t executor;
    struct nros_node_t node;
    struct nros_support_t* support;
} rig_t;

/* One entity of any of the four kinds. Each cycle owns one; the control keeps
 * CONTROL_SLOTS alive in static storage, because the entries a fini-only
 * teardown leaves behind keep naming them. */
typedef union {
    struct nros_subscription_t sub;
    struct nros_timer_t timer;
    struct nros_service_t service;
    struct nros_client_t client;
} entity_t;

typedef struct {
    const char* name;
    /* Entities this kind creates in the backend (a timer creates none). */
    int32_t live_per_entity;
    nros_ret_t (*init)(entity_t*, rig_t*);
    nros_ret_t (*add)(rig_t*, entity_t*);
    nros_ret_t (*remove)(rig_t*, entity_t*);
    nros_ret_t (*fini)(entity_t*);
} kind_t;

/* ---- subscription ------------------------------------------------------- */

static nros_ret_t sub_init(entity_t* e, rig_t* rig) {
    e->sub = rcl_get_zero_initialized_subscription();
    return rclc_subscription_init_default(&e->sub, &rig->node, &MSG_TYPE, "/remove_cycles");
}
static nros_ret_t sub_add(rig_t* rig, entity_t* e) {
    return nros_executor_add_subscription_raw(&rig->executor, &e->sub, on_message, NULL,
                                              NROS_EXECUTOR_ON_NEW_DATA);
}
static nros_ret_t sub_remove(rig_t* rig, entity_t* e) {
    return nros_executor_remove_subscription(&rig->executor, &e->sub);
}
static nros_ret_t sub_fini(entity_t* e) {
    return nros_subscription_fini(&e->sub);
}

/* ---- timer -------------------------------------------------------------- */

static nros_ret_t timer_init(entity_t* e, rig_t* rig) {
    e->timer = rcl_get_zero_initialized_timer();
    return nros_timer_init(&e->timer, rig->support, 1000000000ULL, on_timer, NULL);
}
static nros_ret_t timer_add(rig_t* rig, entity_t* e) {
    return rclc_executor_add_timer(&rig->executor, &e->timer);
}
static nros_ret_t timer_remove(rig_t* rig, entity_t* e) {
    return nros_executor_remove_timer(&rig->executor, &e->timer);
}
static nros_ret_t timer_fini(entity_t* e) {
    return rcl_timer_fini(&e->timer);
}

/* ---- service ------------------------------------------------------------ */

static nros_ret_t service_init(entity_t* e, rig_t* rig) {
    e->service = rcl_get_zero_initialized_service();
    return rclc_service_init_default(&e->service, &rig->node, &SRV_TYPE, "/remove_cycles");
}
static nros_ret_t service_add(rig_t* rig, entity_t* e) {
    return nros_executor_add_service_raw(&rig->executor, &e->service, on_request, NULL);
}
static nros_ret_t service_remove(rig_t* rig, entity_t* e) {
    return nros_executor_remove_service(&rig->executor, &e->service);
}
static nros_ret_t service_fini(entity_t* e) {
    return nros_service_fini(&e->service);
}

/* ---- client ------------------------------------------------------------- */

static nros_ret_t client_init(entity_t* e, rig_t* rig) {
    e->client = rcl_get_zero_initialized_client();
    return rclc_client_init_default(&e->client, &rig->node, &SRV_TYPE, "/remove_cycles");
}
static nros_ret_t client_add(rig_t* rig, entity_t* e) {
    return nros_executor_add_client(&rig->executor, &e->client);
}
static nros_ret_t client_remove(rig_t* rig, entity_t* e) {
    return nros_executor_remove_client(&rig->executor, &e->client);
}
static nros_ret_t client_fini(entity_t* e) {
    return nros_client_fini(&e->client);
}

static const kind_t KINDS[] = {
    {"subscription", 1, sub_init, sub_add, sub_remove, sub_fini},
    {"timer", 0, timer_init, timer_add, timer_remove, timer_fini},
    {"service", 1, service_init, service_add, service_remove, service_fini},
    {"client", 1, client_init, client_add, client_remove, client_fini},
};

static void rig_open(rig_t* rig, struct nros_support_t* support, const char* node_name) {
    rig->support = support;
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

/* ---- positive: add / remove / fini until twice the arena has cycled ----- */

static void cycles(const kind_t* k, struct nros_support_t* support) {
    rig_t rig;
    rig_open(&rig, support, "remove_cycles");

    const size_t capacity = nros_executor_get_arena_capacity(&rig.executor);
    const size_t base_used = nros_executor_get_arena_used(&rig.executor);
    const int32_t base_live = nros_stub_rmw_live_entities();
    CHECK(capacity > 0, "the arena reports a capacity");

    size_t high_water = 0;
    size_t entry = 0;
    int target = MIN_CYCLES;
    int completed = 0;
    for (int i = 0; i < target; i++) {
        entity_t e;
        nros_ret_t r = k->init(&e, &rig);
        if (r != NROS_RET_OK) {
            fprintf(stderr, "FAIL: %s cycle %d: init returned %d\n", k->name, i, (int)r);
            s_failures++;
            break;
        }
        r = k->add(&rig, &e);
        if (r != NROS_RET_OK) {
            fprintf(stderr, "FAIL: %s cycle %d: add returned %d\n", k->name, i, (int)r);
            s_failures++;
            break;
        }
        if (i == 0) {
            high_water = nros_executor_get_arena_used(&rig.executor);
            CHECK(high_water > base_used, "the first registration claimed arena bytes");
            entry = high_water - base_used;
            /* Enough cycles to claim the whole arena twice over. */
            size_t want = entry > 0 ? 2 * capacity / entry + 1 : MIN_CYCLES;
            if (want > (size_t)target) {
                target = want > MAX_CYCLES ? MAX_CYCLES : (int)want;
            }
            CHECK(nros_stub_rmw_live_entities() - base_live == k->live_per_entity,
                  "the entity reached the backend (or, for a timer, did not)");
        } else if (nros_executor_get_arena_used(&rig.executor) != high_water) {
            fprintf(stderr,
                    "FAIL: %s cycle %d: arena used %zu, first cycle %zu -- the released "
                    "region was not reused\n",
                    k->name, i, nros_executor_get_arena_used(&rig.executor), high_water);
            s_failures++;
            break;
        }
        CHECK(nros_executor_get_handle_count(&rig.executor) == 1, "one handle while registered");

        CHECK_RET(k->remove(&rig, &e), NROS_RET_OK, "remove");
        if (nros_executor_get_handle_count(&rig.executor) != 0 ||
            nros_stub_rmw_live_entities() != base_live) {
            fprintf(stderr, "FAIL: %s cycle %d: after remove handle_count=%d live=%d (base %d)\n",
                    k->name, i, nros_executor_get_handle_count(&rig.executor),
                    nros_stub_rmw_live_entities(), base_live);
            s_failures++;
            break;
        }
        if (i == 0) {
            CHECK_RET(k->remove(&rig, &e), NROS_RET_NOT_FOUND, "a second remove finds nothing");
        }
        CHECK_RET(k->fini(&e), NROS_RET_OK, "fini after remove");
        completed++;
    }
    CHECK(completed == target, "every cycle completed");
    CHECK(target > MAX_HANDLES && (size_t)target * entry > capacity,
          "precondition: the loop out-runs both the handle table and the arena");
    printf("  %-12s %6d cycles, arena used pinned at %zu of %zu (entry %zu bytes), "
           "live entities back to %d\n",
           k->name, completed, high_water, capacity, entry, base_live);
    rig_close(&rig);
}

/* ---- negative control: the pre-1631 teardown exhausts ------------------- */

static void fini_alone_exhausts(const kind_t* k, struct nros_support_t* support) {
    rig_t rig;
    rig_open(&rig, support, "fini_alone");
    const int32_t base_live = nros_stub_rmw_live_entities();

    static entity_t entities[CONTROL_SLOTS];
    int failed_at = -1;
    nros_ret_t failed_with = NROS_RET_OK;
    for (int i = 0; i < CONTROL_SLOTS; i++) {
        CHECK_RET(k->init(&entities[i], &rig), NROS_RET_OK, "control: init");
        nros_ret_t r = k->add(&rig, &entities[i]);
        if (r != NROS_RET_OK) {
            failed_at = i;
            failed_with = r;
            (void)k->fini(&entities[i]);
            break;
        }
        /* fini WITHOUT remove — the teardown every C image used before 1631. */
        CHECK_RET(k->fini(&entities[i]), NROS_RET_OK, "control: fini");
    }
    CHECK(failed_at >= 0, "an executor that never runs out proves nothing: fini alone must "
                          "exhaust it, or the positive loop measured nothing");
    CHECK(failed_at <= MAX_HANDLES, "and it runs out at the handle table");
    CHECK(nros_stub_rmw_live_entities() - base_live == k->live_per_entity * failed_at,
          "every fini'd-but-not-removed entity is still on the graph");
    printf("  %-12s control: fini alone failed add #%d with %d; %d entities still live\n", k->name,
           failed_at, (int)failed_with, nros_stub_rmw_live_entities() - base_live);
    rig_close(&rig);
    CHECK(nros_stub_rmw_live_entities() == base_live,
          "rclc_executor_fini drops every entry it still held");
}

/* ---- a remove on the wrong executor is NOT_FOUND ----------------------- */

static void wrong_executor_is_not_found(const kind_t* k, struct nros_support_t* support) {
    rig_t a;
    rig_t b;
    rig_open(&a, support, "remove_a");
    rig_open(&b, support, "remove_b");
    entity_t e;
    CHECK_RET(k->init(&e, &a), NROS_RET_OK, "init");
    CHECK_RET(k->add(&a, &e), NROS_RET_OK, "add on A");
    CHECK_RET(k->remove(&b, &e), NROS_RET_NOT_FOUND, "remove on B finds nothing");
    CHECK(nros_executor_get_handle_count(&a.executor) == 1, "and A still holds it");
    CHECK_RET(k->remove(&a, &e), NROS_RET_OK, "remove on A");
    CHECK_RET(k->fini(&e), NROS_RET_OK, "fini");
    rig_close(&b);
    rig_close(&a);
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct nros_support_t support = nros_support_get_zero_initialized();
    CHECK_RET(nros_support_init_rmw(&support, "stub://none", 46, "entity_remove_cycles",
                                    NROS_STUB_RMW_NAME),
              NROS_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) {
        return 1;
    }

    for (size_t i = 0; i < sizeof KINDS / sizeof KINDS[0]; i++) {
        cycles(&KINDS[i], &support);
        fini_alone_exhausts(&KINDS[i], &support);
        wrong_executor_is_not_found(&KINDS[i], &support);
    }

    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "entity_remove_cycles: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("entity_remove_cycles: OK\n");
    return 0;
}
