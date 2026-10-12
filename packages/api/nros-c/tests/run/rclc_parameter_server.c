/* phase-491 (RFC-0105) — rclc's parameter server, over the executor's store.
 *
 * COMPILE-AND-RUN, for the reason the probes before it gave: the contract is a
 * BEHAVIOUR. A callback that fired on `rclc_add_parameter`, a local set that
 * obeyed read-only, a second node sharing the first node's table, a server
 * accepting a node bound to no executor — every one of those compiles and
 * links, and only a run says it differs from rclc.
 *
 * What is pinned, against rclc's own `parameter_server.c` (humble + rolling):
 *  - init refuses a node bound to no executor (RFC-0105 D3);
 *  - the callback fires on local typed sets with (old, new), and NOT on
 *    `rclc_add_parameter` or a local `rclc_delete_parameter`;
 *  - `false` from it refuses with RCLC_PARAMETER_MODIFICATION_REJECTED and
 *    leaves the value alone;
 *  - inside it, mutating calls answer RCLC_PARAMETER_DISABLED_ON_CALLBACK and
 *    gets still work;
 *  - local sets skip read-only and range (rclc never checks them locally);
 *  - a typed set of the wrong type is RCLC_PARAMETER_TYPE_MISMATCH;
 *  - `max_params` is a per-node quota;
 *  - two nodes' servers keep two tables;
 *  - `fini` takes the node's parameters with it.
 *
 * The remote half (range and read-only enforced, NOT_SET deletes through the
 * callback, undeclared creates with old == NULL) is `nros-params`'s
 * `apply` and is unit-tested there; this probe has no wire.
 *
 * The RMW backend is the shared stub, switched to ACCEPT entities so the six
 * parameter services `rclc_executor_add_parameter_server` registers can be
 * created.
 */

#include <nros/nros.h>
#include <nros/rmw_vtable.h>
#include <nros/rcl_compat.h>
#include <rclc_parameter/rclc_parameter.h>

#include <stdio.h>
#include <string.h>

#include "stub_rmw_backend.h"

static int g_failures;

#define CHECK(cond, ...)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            g_failures++;                                                                          \
            printf("FAIL %s:%d: ", __FILE__, __LINE__);                                            \
            printf(__VA_ARGS__);                                                                   \
            printf("\n");                                                                          \
        }                                                                                          \
    } while (0)

void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

/* ---- the callback under test ------------------------------------------- */

static struct {
    int calls;
    bool saw_old;
    bool saw_new;
    int64_t old_int;
    int64_t new_int;
    char name[32];
    rcl_ret_t nested_set;
    rcl_ret_t nested_get;
    int64_t nested_got;
    rclc_parameter_server_t* server;
} g_cb;

static bool on_change(const Parameter* old_param, const Parameter* new_param, void* context) {
    CHECK(context == &g_cb, "the callback's context is not the one registered");
    g_cb.calls++;
    g_cb.saw_old = old_param != NULL;
    g_cb.saw_new = new_param != NULL;
    const Parameter* p = new_param != NULL ? new_param : old_param;
    if (p != NULL) {
        snprintf(g_cb.name, sizeof(g_cb.name), "%s", p->name.data);
    }
    if (old_param != NULL) g_cb.old_int = old_param->value.integer_value;
    if (new_param != NULL) g_cb.new_int = new_param->value.integer_value;
    /* rclc refuses mutation from inside the callback, and allows reads. */
    g_cb.nested_set = rclc_parameter_set_int(g_cb.server, "other", 1);
    g_cb.nested_get = rclc_parameter_get_int(g_cb.server, "other", &g_cb.nested_got);
    /* Refuse exactly one value. */
    return new_param == NULL || new_param->value.integer_value != 13;
}

int main(void) {
    struct rclc_support_t support;
    struct rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    struct rcl_node_t talker, listener, unbound;
    rclc_parameter_server_t ps, ps2, ps3;
    int64_t got = 0;

    memset(&support, 0, sizeof(support));
    memset(&talker, 0, sizeof(talker));
    memset(&listener, 0, sizeof(listener));
    memset(&unbound, 0, sizeof(unbound));
    memset(&ps, 0, sizeof(ps));
    memset(&ps2, 0, sizeof(ps2));
    memset(&ps3, 0, sizeof(ps3));
    memset(&g_cb, 0, sizeof(g_cb));

    nros_stub_rmw_set_accept_entities(true);
    CHECK(rclc_support_init_rmw(&support, NULL, 0, "rclc_param_server", NROS_STUB_RMW_NAME) ==
              RCL_RET_OK,
          "support init");
    CHECK(rclc_executor_init(&executor, &support.context, 8, NULL) == RCL_RET_OK, "executor init");

    /* D3 — a node bound to no executor has no store to serve from. */
    CHECK(rclc_parameter_server_init_default(&ps, &unbound) == RCL_RET_NOT_INIT,
          "init accepted a node that is bound to no executor");

    CHECK(rclc_executor_node_init(&executor, &talker, "talker", NULL) == RCL_RET_OK,
          "talker node init");
    CHECK(rclc_executor_node_init(&executor, &listener, "listener", NULL) == RCL_RET_OK,
          "listener node init");
    if (g_failures != 0) {
        printf("rclc_parameter_server: %d setup failure(s)\n", g_failures);
        return 1;
    }

    rclc_parameter_options_t opts = {.notify_changed_over_dds = false,
                                     .max_params = 8,
                                     .allow_undeclared_parameters = false,
                                     .low_mem_mode = false};
    CHECK(rclc_parameter_server_init_with_option(&ps, &talker, &opts) == RCL_RET_OK,
          "server init on talker");
    g_cb.server = &ps;
    CHECK(rclc_executor_add_parameter_server_with_context(&executor, &ps, on_change, &g_cb) ==
              RCL_RET_OK,
          "add the server to its executor (registers the six services)");

    /* rclc_add_parameter declares the zero value and fires no callback. */
    CHECK(rclc_add_parameter(&ps, "rate", RCLC_PARAMETER_INT) == RCL_RET_OK, "add rate");
    CHECK(rclc_add_parameter(&ps, "other", RCLC_PARAMETER_INT) == RCL_RET_OK, "add other");
    CHECK(rclc_parameter_get_int(&ps, "rate", &got) == RCL_RET_OK && got == 0,
          "an added INT reads %lld, expected rclc's zero", (long long)got);
    CHECK(g_cb.calls == 0, "rclc_add_parameter fired the callback (%d)", g_cb.calls);

    /* A local typed set fires (old, new) BEFORE the write. */
    CHECK(rclc_parameter_set_int(&ps, "rate", 10) == RCL_RET_OK, "set rate 10");
    CHECK(g_cb.calls == 1 && g_cb.saw_old && g_cb.saw_new, "a local set must fire with old+new");
    CHECK(g_cb.old_int == 0 && g_cb.new_int == 10, "callback saw old=%lld new=%lld",
          (long long)g_cb.old_int, (long long)g_cb.new_int);
    CHECK(strcmp(g_cb.name, "rate") == 0, "callback saw name `%s`", g_cb.name);
    CHECK(g_cb.nested_set == RCLC_PARAMETER_DISABLED_ON_CALLBACK,
          "a set from inside the callback returned %d", (int)g_cb.nested_set);
    CHECK(g_cb.nested_get == RCL_RET_OK, "a get from inside the callback returned %d",
          (int)g_cb.nested_get);

    /* The veto leaves the value alone. */
    CHECK(rclc_parameter_set_int(&ps, "rate", 13) == RCLC_PARAMETER_MODIFICATION_REJECTED,
          "the callback's `false` was not reported as MODIFICATION_REJECTED");
    CHECK(rclc_parameter_get_int(&ps, "rate", &got) == RCL_RET_OK && got == 10,
          "a refused set moved the value to %lld", (long long)got);

    /* Wrong type, missing name. */
    CHECK(rclc_parameter_set_double(&ps, "rate", 1.5) == RCLC_PARAMETER_TYPE_MISMATCH,
          "a typed set of the wrong type was not TYPE_MISMATCH");
    CHECK(rclc_parameter_set_int(&ps, "missing", 1) == RCL_RET_ERROR,
          "a set of an undeclared name was not RCL_RET_ERROR");
    double d = 0;
    CHECK(rclc_parameter_get_double(&ps, "rate", &d) == RCL_RET_INVALID_ARGUMENT,
          "a typed get of the wrong type was not INVALID_ARGUMENT");

    /* Local sets ignore read-only and range, as rclc's do. */
    CHECK(rclc_add_parameter_constraint_integer(&ps, "rate", 1, 20, 1) == RCL_RET_OK,
          "attach a range");
    CHECK(rclc_set_parameter_read_only(&ps, "rate", true) == RCL_RET_OK, "mark read-only");
    CHECK(rclc_parameter_set_int(&ps, "rate", 50) == RCL_RET_OK,
          "the OWNER's local set was refused by read-only or range");
    int64_t from = 0, to = 0, step = 0;
    CHECK(rclc_parameter_get_integer_range(&ps, "rate", &from, &to, &step) == RCL_RET_OK &&
              from == 1 && to == 20 && step == 1,
          "range read back as [%lld, %lld] step %lld", (long long)from, (long long)to,
          (long long)step);
    bool ro = false;
    CHECK(rclc_parameter_describe(&ps, "rate", NULL, 0, NULL, 0, &ro) == RCL_RET_OK && ro,
          "describe did not report read-only");

    /* Local delete fires no callback. */
    int before = g_cb.calls;
    CHECK(rclc_delete_parameter(&ps, "other") == RCL_RET_OK, "delete other");
    CHECK(g_cb.calls == before, "a local delete fired the callback");
    CHECK(rclc_parameter_get_type(&ps, "other") == RCLC_PARAMETER_NOT_SET,
          "a deleted parameter is still declared");

    /* Extensions: string and arrays round-trip through caller buffers. */
    char text[32];
    CHECK(rclc_parameter_declare_string(&ps, "frame", "base_link") == RCL_RET_OK, "declare frame");
    CHECK(rclc_parameter_get_string(&ps, "frame", text, sizeof(text)) == RCL_RET_OK &&
              strcmp(text, "base_link") == 0,
          "frame read back as `%s`", text);
    CHECK(rclc_parameter_get_string(&ps, "frame", text, 4) == RCLC_RET_FULL,
          "a too-small buffer was not RCLC_RET_FULL");
    const double gains[3] = {1.0, 2.0, 3.0};
    double out[4] = {0};
    size_t n = 0;
    CHECK(rclc_parameter_declare_double_array(&ps, "gains", gains, 3) == RCL_RET_OK,
          "declare gains");
    CHECK(rclc_parameter_get_double_array(&ps, "gains", out, 4, &n) == RCL_RET_OK && n == 3 &&
              out[2] == 3.0,
          "gains read back n=%zu", n);

    const char* frames[2] = {"map", "odom"};
    char rows[3][8];
    CHECK(rclc_parameter_declare_string_array(&ps, "frames", frames, 2) == RCL_RET_OK,
          "declare frames");
    CHECK(rclc_parameter_get_string_array(&ps, "frames", &rows[0][0], 8, 3, &n) == RCL_RET_OK &&
              n == 2 && strcmp(rows[1], "odom") == 0,
          "frames read back n=%zu `%s`", n, rows[1]);
    CHECK(rclc_parameter_get_string_array(&ps, "frames", &rows[0][0], 3, 3, &n) == RCLC_RET_FULL,
          "a row too short for `odom` was not RCLC_RET_FULL");

    /* declare_* keeps a value already declared with the same type (how a baked
     * launch value survives), and refuses a different type. */
    CHECK(rclc_parameter_declare_int(&ps, "rate", 99) == RCL_RET_OK,
          "re-declaring with the same type was refused");
    CHECK(rclc_parameter_get_int(&ps, "rate", &got) == RCL_RET_OK && got == 50,
          "re-declaring overwrote the value (%lld)", (long long)got);
    CHECK(rclc_parameter_declare_bool(&ps, "rate", true) == RCL_RET_ERROR,
          "re-declaring with a different type was accepted");

    /* A second node's server keeps its own table, and its own quota. */
    rclc_parameter_options_t small = opts;
    small.max_params = 2;
    CHECK(rclc_parameter_server_init_with_option(&ps2, &listener, &small) == RCL_RET_OK,
          "server init on listener");
    CHECK(rclc_parameter_declare_int(&ps2, "rate", 20) == RCL_RET_OK, "listener rate");
    CHECK(rclc_parameter_get_int(&ps2, "rate", &got) == RCL_RET_OK && got == 20,
          "listener read %lld", (long long)got);
    CHECK(rclc_parameter_get_int(&ps, "rate", &got) == RCL_RET_OK && got == 50,
          "listener's declare moved talker's value to %lld", (long long)got);
    CHECK(rclc_parameter_declare_int(&ps2, "b", 0) == RCL_RET_OK, "listener second param");
    CHECK(rclc_parameter_declare_int(&ps2, "c", 0) == RCL_RET_ERROR,
          "max_params=2 accepted a third parameter");

    size_t count = 0;
    CHECK(rclc_parameter_list(&ps, NULL, NULL, 0, 0, &count) == RCLC_RET_FULL || count == 0,
          "list count call");
    CHECK(count == 4, "talker lists %zu parameters, expected rate/frame/gains/frames", count);

    /* add_parameter_server checks the executor is the node's. */
    struct rclc_executor_t other = rclc_executor_get_zero_initialized_executor();
    CHECK(rclc_executor_add_parameter_server(&other, &ps2, NULL) == RCL_RET_INVALID_ARGUMENT,
          "a server was added to an executor its node is not bound to");

    /* fini takes the node's parameters with it. */
    CHECK(rclc_parameter_server_fini(&ps2, &listener) == RCL_RET_OK, "fini listener");
    CHECK(rclc_parameter_server_init_with_option(&ps3, &listener, &small) == RCL_RET_OK,
          "re-init listener");
    CHECK(rclc_parameter_get_type(&ps3, "rate") == RCLC_PARAMETER_NOT_SET,
          "fini left the node's parameters behind");

    if (g_failures != 0) {
        printf("rclc_parameter_server: %d failure(s)\n", g_failures);
        return 1;
    }
    printf("rclc_parameter_server: one rclc family, one store\n");
    return 0;
}
