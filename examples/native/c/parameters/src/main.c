/// @file main.c
/// @brief C parameters example — rclc's parameter server, served to `ros2 param`.
///
/// phase-491 (RFC-0105). The C parameter API is rclc's
/// `rclc_parameter_server_t`, under rclc's names and at rclc's include path,
/// over the executor's one parameter store — the store the six
/// `rcl_interfaces/srv/*` servers read. Run it and, while it spins, ask ROS 2:
///
/// ```console
/// $ NROS_SPIN_MS=60000 ./c_parameters &
/// $ ros2 param list /c_parameters
/// $ ros2 param get  /c_parameters scale_factor
/// $ ros2 param set  /c_parameters scale_factor 2.5
/// ```
///
/// (Add `--no-daemon` if a `ros2` daemon from an earlier session is still
/// holding a stale graph — it caches across domains and answers "Node not
/// found" for a node that is right there.)
///
/// Two nano-ros rules, both stated in the header: the node is bound to the
/// executor (`rclc_executor_node_init`) BEFORE the server is initialised, and
/// nothing here allocates. The example exits 0 only when every roundtrip
/// passes; a non-zero exit code encodes which assertion failed. Consumed by
/// the `parameters_roundtrip` test.

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/clock.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <rclc_parameter/rclc_parameter.h>

/* Static allocation — all nros structs live in .bss, not on the stack. */
static struct {
    rclc_support_t support;
    rclc_executor_t executor;
    rcl_node_t node;
    rclc_parameter_server_t params;
} app;

/* rclc's callback: `old` and `new` are the two sides of every change, local or
 * remote. Refuse a non-positive rate, as a real node would. */
static unsigned g_changes;
static bool on_parameter_change(const Parameter* old_param, const Parameter* new_param,
                                void* context) {
    (void)old_param;
    (void)context;
    g_changes++;
    if (new_param != NULL && new_param->value.type == RCLC_PARAMETER_INT &&
        strcmp(new_param->name.data, "publish_rate_hz") == 0) {
        return new_param->value.integer_value > 0;
    }
    return true;
}

/* How long to keep spinning after the roundtrip, so the six parameter services
 * this node published have something to answer with. 0 exits immediately. */
static unsigned spin_ms(void) {
    const char* s = getenv("NROS_SPIN_MS");
    if (s == NULL || *s == '\0') {
        return 2000u;
    }
    long v = strtol(s, NULL, 10);
    return v > 0 ? (unsigned)v : 0u;
}

static int run(void) {
    /* Clock demo: read the system clock once. */
    rcl_clock_t clock;
    if (rcl_clock_init(RCLC_CLOCK_SYSTEM_TIME, &clock, NULL) == RCL_RET_OK) {
        rclc_time_t now;
        if (rclc_clock_get_now_time(&clock, &now) == RCL_RET_OK) {
            printf("System time: %d.%09u sec\n", now.sec, now.nanosec);
        }
        (void)rcl_clock_fini(&clock);
    }

    const char* locator = getenv("NROS_LOCATOR");
    if (!locator) {
        locator = NROS_ENTRY_LOCATOR;
    }
    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = (uint8_t)NROS_ENTRY_DOMAIN_ID;
    if (domain_str) {
        domain_id = (uint8_t)atoi(domain_str);
    }

    memset(&app, 0, sizeof(app));
    RCLC_CHECK_RET(rclc_support_init_with_locator(&app.support, locator, domain_id), 1);
    /* 16 handles: the six rcl_interfaces parameter services plus headroom. */
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 16, NULL), 1);
    /* The namespace is explicit: NULL options leave it EMPTY, and an empty
     * namespace is not the root one. */
    rcl_node_options_t node_opts = rcl_node_get_default_options();
    node_opts.namespace_[0] = (uint8_t)'/';
    node_opts.namespace_len = 1;
    RCLC_CHECK_RET(rclc_executor_node_init(&app.executor, &app.node, "c_parameters", &node_opts),
                   1);

    /* The server handle. The node is already bound to the executor, which is
     * what the server needs: the store it declares into is the executor's. */
    rclc_parameter_options_t opts = {.notify_changed_over_dds = true,
                                     .max_params = 8,
                                     .allow_undeclared_parameters = false,
                                     .low_mem_mode = false};
    RCLC_CHECK_RET(rclc_parameter_server_init_with_option(&app.params, &app.node, &opts), 1);
    /* The six rcl_interfaces parameter services, plus the callback. */
    RCLC_CHECK_RET(
        rclc_executor_add_parameter_server(&app.executor, &app.params, on_parameter_change), 1);

    /* Declare with code defaults. `rclc_parameter_declare_*` keeps a launch
     * value baked under the same name; nothing seeds a standalone image, so the
     * defaults win here. (rclc's `rclc_add_parameter` + `rclc_parameter_set_*`
     * works too, and overwrites a launch value, as upstream.) */
    if (rclc_parameter_declare_bool(&app.params, "verbose", false) != RCL_RET_OK) return 1;
    if (rclc_parameter_declare_int(&app.params, "publish_rate_hz", 1) != RCL_RET_OK) return 1;
    if (rclc_parameter_declare_double(&app.params, "scale_factor", 1.0) != RCL_RET_OK) return 1;
    if (rclc_parameter_declare_string(&app.params, "topic_name", "/chatter") != RCL_RET_OK)
        return 1;
    (void)rclc_add_parameter_constraint_integer(&app.params, "publish_rate_hz", 1, 100, 1);

    bool verbose = true;
    int64_t rate_hz = 0;
    double scale = 0.0;
    char topic[64] = {0};

    if (rclc_parameter_get_bool(&app.params, "verbose", &verbose) != RCL_RET_OK) return 2;
    if (rclc_parameter_get_int(&app.params, "publish_rate_hz", &rate_hz) != RCL_RET_OK) return 2;
    if (rclc_parameter_get_double(&app.params, "scale_factor", &scale) != RCL_RET_OK) return 2;
    if (rclc_parameter_get_string(&app.params, "topic_name", topic, sizeof(topic)) != RCL_RET_OK)
        return 2;

    printf("Parameters: verbose=%s, rate=%lld Hz, scale=%.2f, topic=%s\n",
           verbose ? "true" : "false", (long long)rate_hz, scale, topic);

    if (verbose != false) return 3;
    if (rate_hz != 1) return 3;
    if (scale < 0.99 || scale > 1.01) return 3;
    if (strcmp(topic, "/chatter") != 0) return 3;

    /* Local sets fire the callback, as rclc's do. */
    if (rclc_parameter_set_bool(&app.params, "verbose", true) != RCL_RET_OK) return 4;
    if (rclc_parameter_get_bool(&app.params, "verbose", &verbose) != RCL_RET_OK) return 4;
    if (verbose != true) return 4;
    printf("After set: verbose=%s\n", verbose ? "true" : "false");

    if (rclc_parameter_set_int(&app.params, "publish_rate_hz", 10) != RCL_RET_OK) return 4;
    if (rclc_parameter_get_int(&app.params, "publish_rate_hz", &rate_hz) != RCL_RET_OK) return 4;
    if (rate_hz != 10) return 4;

    if (rclc_parameter_set_string(&app.params, "topic_name", "/rosout") != RCL_RET_OK) return 4;
    if (rclc_parameter_get_string(&app.params, "topic_name", topic, sizeof(topic)) != RCL_RET_OK)
        return 4;
    if (strcmp(topic, "/rosout") != 0) return 4;
    /* Read-only to the OUTSIDE, as rclc means it: a `ros2 param set` on it is
     * refused from now on, while this node's own sets still go through. */
    if (rclc_set_parameter_read_only(&app.params, "topic_name", true) != RCL_RET_OK) return 4;

    /* The callback's veto, and a typed set of the wrong type. */
    if (rclc_parameter_set_int(&app.params, "publish_rate_hz", 0) !=
        RCLC_PARAMETER_MODIFICATION_REJECTED)
        return 5;
    if (rclc_parameter_set_double(&app.params, "publish_rate_hz", 2.0) !=
        RCLC_PARAMETER_TYPE_MISMATCH)
        return 5;
    if (g_changes < 4) return 5;

    /* Unknown parameters are refused, not invented — on read AND on write. */
    if (rclc_parameter_get_bool(&app.params, "missing", &verbose) == RCL_RET_OK) return 6;
    if (rclc_parameter_get_type(&app.params, "missing") != RCLC_PARAMETER_NOT_SET) return 6;
    if (rclc_parameter_set_bool(&app.params, "missing", true) == RCL_RET_OK) return 6;

    printf("OK verbose=%s rate=%lld topic=%s\n", verbose ? "true" : "false", (long long)rate_hz,
           topic);

    /* Answer the parameter services for a while. */
    const unsigned budget = spin_ms();
    for (unsigned waited = 0; waited < budget; waited += 100) {
        (void)rclc_executor_spin_some(&app.executor, 100ULL * 1000ULL * 1000ULL);
    }

    (void)rclc_parameter_server_fini(&app.params, &app.node);
    rclc_executor_fini(&app.executor);
    rclc_support_fini(&app.support);
    return 0;
}

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif
    return run();
}

NROS_APP_MAIN_REGISTER()
