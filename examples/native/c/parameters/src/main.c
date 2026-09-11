/// @file main.c
/// @brief C parameters example — declare / get / set on THE parameter store.
///
/// phase-426 W4. This example used to build an `nros_parameter_server_t` over a
/// static `nros_parameter_t[8]` and exercise that. It is a real API and it
/// still exists, but it is a SECOND store: the six `rcl_interfaces/srv/*`
/// servers read the `nros_params` table the executor owns, so every parameter
/// this file declared was invisible to `ros2 param get` — the defect phase-426
/// exists to remove, shipped in the example a user copies out. Same demo,
/// against the one store, through `nros_executor_*_param_*_on`.
///
/// Parameters belong to a NODE, so the example opens a session, an executor
/// and a node. Run it and, while it spins, ask ROS 2:
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
/// The `_on` spellings name the node explicitly (phase-426 W1 keys the store by
/// node, so two nodes on one executor may declare the same name); the
/// un-suffixed ones mean the primary node and are what a single-node image
/// writes. The example exits 0 only when every roundtrip passes; a non-zero
/// exit code encodes which assertion failed. Consumed by the
/// `parameters_roundtrip` test.

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
#include <nros/parameter.h>

/* Static allocation — all nros structs live in .bss, not on the stack. */
static struct {
    nros_support_t support;
    nros_executor_t executor;
    nros_node_t node;
} app;

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
    nros_clock_t clock;
    if (nros_clock_init(&clock, NROS_CLOCK_SYSTEM_TIME) == NROS_RET_OK) {
        nros_time_t now;
        if (nros_clock_get_now(&clock, &now) == NROS_RET_OK) {
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
    NROS_CHECK_RET(nros_support_init(&app.support, locator, domain_id), 1);
    /* 16 handles: the six `rcl_interfaces/srv/*` servers plus headroom. */
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 16), 1);
    /* The namespace is explicit: NULL options leave it EMPTY, and an empty
     * namespace is not the root one. */
    nros_node_options_t node_opts = rcl_node_get_default_options();
    node_opts.namespace_[0] = (uint8_t)'/';
    node_opts.namespace_len = 1;
    NROS_CHECK_RET(nros_executor_node_init(&app.executor, &app.node, "c_parameters", &node_opts),
                   1);

    /* The six `rcl_interfaces/srv/*` servers. Without this the parameters are
     * still in the right store — they are simply not reachable from outside
     * the image. */
    NROS_CHECK_RET(nros_executor_register_parameter_services(&app.executor), 1);

    /* Declare with code defaults. A launch `<param>` would have seeded the
     * store first and the declare would adopt it; nothing seeds a standalone
     * image, so the defaults win here. */
    if (nros_executor_declare_param_bool_on(&app.executor, &app.node, "verbose", false) !=
        NROS_RET_OK)
        return 1;
    if (nros_executor_declare_param_integer_on(&app.executor, &app.node, "publish_rate_hz", 1) !=
        NROS_RET_OK)
        return 1;
    if (nros_executor_declare_param_double_on(&app.executor, &app.node, "scale_factor", 1.0) !=
        NROS_RET_OK)
        return 1;
    if (nros_executor_declare_param_string_on(&app.executor, &app.node, "topic_name", "/chatter") !=
        NROS_RET_OK)
        return 1;

    bool verbose = true;
    int64_t rate_hz = 0;
    double scale = 0.0;
    char topic[64] = {0};

    if (nros_executor_get_param_bool_on(&app.executor, &app.node, "verbose", &verbose) !=
        NROS_RET_OK)
        return 2;
    if (nros_executor_get_param_integer_on(&app.executor, &app.node, "publish_rate_hz", &rate_hz) !=
        NROS_RET_OK)
        return 2;
    if (nros_executor_get_param_double_on(&app.executor, &app.node, "scale_factor", &scale) !=
        NROS_RET_OK)
        return 2;
    if (nros_executor_get_param_string_on(&app.executor, &app.node, "topic_name", topic,
                                          sizeof(topic)) != NROS_RET_OK)
        return 2;

    printf("Parameters: verbose=%s, rate=%lld Hz, scale=%.2f, topic=%s\n",
           verbose ? "true" : "false", (long long)rate_hz, scale, topic);

    if (verbose != false) return 3;
    if (rate_hz != 1) return 3;
    if (scale < 0.99 || scale > 1.01) return 3;
    if (strcmp(topic, "/chatter") != 0) return 3;

    /* A set is `ParameterServer::apply` on the Rust side — the same entry point
     * a remote `ros2 param set` reaches, so the read-only / type / range rules
     * are one implementation, not two. */
    if (nros_executor_set_param_bool_on(&app.executor, &app.node, "verbose", true) != NROS_RET_OK)
        return 4;
    if (nros_executor_get_param_bool_on(&app.executor, &app.node, "verbose", &verbose) !=
        NROS_RET_OK)
        return 4;
    if (verbose != true) return 4;
    printf("After set: verbose=%s\n", verbose ? "true" : "false");

    if (nros_executor_set_param_integer_on(&app.executor, &app.node, "publish_rate_hz", 10) !=
        NROS_RET_OK)
        return 4;
    if (nros_executor_get_param_integer_on(&app.executor, &app.node, "publish_rate_hz", &rate_hz) !=
        NROS_RET_OK)
        return 4;
    if (rate_hz != 10) return 4;

    if (nros_executor_set_param_string_on(&app.executor, &app.node, "topic_name", "/rosout") !=
        NROS_RET_OK)
        return 4;
    if (nros_executor_get_param_string_on(&app.executor, &app.node, "topic_name", topic,
                                          sizeof(topic)) != NROS_RET_OK)
        return 4;
    if (strcmp(topic, "/rosout") != 0) return 4;

    /* Unknown parameters must be rejected, not invented — on read AND on write
     * (issue 1151: a set does not create a slot). */
    if (nros_executor_get_param_bool_on(&app.executor, &app.node, "missing", &verbose) ==
        NROS_RET_OK)
        return 5;
    if (nros_executor_has_param_on(&app.executor, &app.node, "missing")) return 5;
    if (nros_executor_set_param_bool_on(&app.executor, &app.node, "missing", true) == NROS_RET_OK)
        return 5;

    /* This node IS the primary one (the first built on this executor), so the
     * un-suffixed spellings name it. Asserting it is what keeps "the two
     * families are one table" from being a claim nobody checks. */
    int64_t primary = 0;
    if (nros_executor_get_param_integer(&app.executor, "publish_rate_hz", &primary) != NROS_RET_OK)
        return 6;
    if (primary != rate_hz) return 6;

    printf("OK verbose=%s rate=%lld topic=%s\n", verbose ? "true" : "false", (long long)rate_hz,
           topic);

    /* Answer the parameter services for a while. */
    const unsigned budget = spin_ms();
    for (unsigned waited = 0; waited < budget; waited += 100) {
        (void)rclc_executor_spin_some(&app.executor, 100ULL * 1000ULL * 1000ULL);
    }

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
