/// @file main.c
/// @brief C lifecycle example — one managed node, driven by `ros2 lifecycle`.
///
/// The node is bound to the executor (`rclc_executor_node_init`), then the five
/// REP-2002 lifecycle services are registered on it. They are served under the
/// node's own name, as a stock rclcpp lifecycle node's are. Run it and, while
/// it spins, drive it from ROS 2:
///
/// ```console
/// $ ./c_lifecycle_node &
/// $ ros2 lifecycle nodes
/// $ ros2 lifecycle get  /c_lifecycle_node
/// $ ros2 lifecycle set  /c_lifecycle_node configure
/// $ ros2 lifecycle set  /c_lifecycle_node activate
/// ```
///
/// (Add `--no-daemon` if a `ros2` daemon from an earlier session still holds a
/// stale graph.)
///
/// Each transition callback prints one line, so a test can see that the
/// node's own code ran. `NROS_SPIN_MS` bounds the run; unset, it spins until
/// killed.
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>

/* Static allocation: every nros struct lives in .bss, not on the stack. */
static struct {
    rclc_support_t support;
    rclc_executor_t executor;
    rcl_node_t node;
} app;

/* The transition callbacks. Each returns a REP-2002 result; all succeed. */
static uint8_t on_configure(void* context) {
    (void)context;
    printf("on_configure\n");
    return RCLC_LIFECYCLE_RET_OK;
}

static uint8_t on_activate(void* context) {
    (void)context;
    printf("on_activate\n");
    return RCLC_LIFECYCLE_RET_OK;
}

static uint8_t on_deactivate(void* context) {
    (void)context;
    printf("on_deactivate\n");
    return RCLC_LIFECYCLE_RET_OK;
}

static uint8_t on_cleanup(void* context) {
    (void)context;
    printf("on_cleanup\n");
    return RCLC_LIFECYCLE_RET_OK;
}

static uint8_t on_shutdown(void* context) {
    (void)context;
    printf("on_shutdown\n");
    return RCLC_LIFECYCLE_RET_OK;
}

/* How long to spin, in ms. Unset or 0: until the process is killed. */
static unsigned spin_ms(void) {
    const char* s = getenv("NROS_SPIN_MS");
    if (s == NULL || *s == '\0') {
        return 0u;
    }
    long v = strtol(s, NULL, 10);
    return v > 0 ? (unsigned)v : 0u;
}

static int run(void) {
    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, 0, NULL, &allocator), 1);
    /* 8 handles: the five lifecycle services plus headroom. */
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 8, NULL), 1);

    /* The namespace is explicit: NULL options leave it EMPTY, and an empty
     * namespace is not the root one. */
    rcl_node_options_t node_opts = rcl_node_get_default_options();
    node_opts.namespace_[0] = (uint8_t)'/';
    node_opts.namespace_len = 1;
    RCLC_CHECK_RET(
        rclc_executor_node_init(&app.executor, &app.node, "c_lifecycle_node", &node_opts), 1);

    /* The five REP-2002 services, on the node bound above:
     * /c_lifecycle_node/get_state, /c_lifecycle_node/change_state, ... */
    RCLC_CHECK_RET(rclc_executor_register_lifecycle_services(&app.executor), 2);
    RCLC_CHECK_RET(rclc_executor_lifecycle_register_on_configure(&app.executor, on_configure, NULL),
                   3);
    RCLC_CHECK_RET(rclc_executor_lifecycle_register_on_activate(&app.executor, on_activate, NULL),
                   3);
    RCLC_CHECK_RET(
        rclc_executor_lifecycle_register_on_deactivate(&app.executor, on_deactivate, NULL), 3);
    RCLC_CHECK_RET(rclc_executor_lifecycle_register_on_cleanup(&app.executor, on_cleanup, NULL), 3);
    RCLC_CHECK_RET(rclc_executor_lifecycle_register_on_shutdown(&app.executor, on_shutdown, NULL),
                   3);

    printf("Lifecycle services registered on /c_lifecycle_node (state %u)\n",
           (unsigned)rclc_executor_lifecycle_get_current_state(&app.executor));
    printf("Ready. Drive it with `ros2 lifecycle set /c_lifecycle_node configure`.\n");

    /* Each spin drains the lifecycle services, so `ros2 lifecycle` round-trips. */
    const unsigned budget = spin_ms();
    for (unsigned waited = 0; budget == 0u || waited < budget; waited += 100) {
        (void)rclc_executor_spin_some(&app.executor, 100ULL * 1000ULL * 1000ULL);
    }

    rclc_executor_fini(&app.executor);
    rclc_support_fini(&app.support);
    return 0;
}

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
#ifdef _IOLBF
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif
    return run();
}
