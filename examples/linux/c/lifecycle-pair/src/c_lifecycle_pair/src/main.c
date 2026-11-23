/// @file main.c
/// @brief Two managed C nodes in one image, each with its own lifecycle.
///
/// `/lc_alpha` and `/lc_beta` are both bound to one executor
/// (`rclc_executor_node_init`). Each gets its own five REP-2002 services and
/// its own state machine (`rclc_node_register_lifecycle_services`), so
/// `ros2 lifecycle` drives one without moving the other:
///
/// ```console
/// $ ./c_lifecycle_pair &
/// $ ros2 lifecycle nodes
/// $ ros2 lifecycle set /lc_alpha configure
/// $ ros2 lifecycle set /lc_alpha activate
/// $ ros2 topic echo /lc_alpha/chatter std_msgs/msg/String
/// ```
///
/// Each node publishes `<node>: <n>` on `/<node>/chatter` from one shared
/// timer. Nothing goes out until that node is `active`: a managed node's
/// publishers follow its state. Publishing stops at `deactivate` and resumes
/// at the next `activate`. Each `on_activate` also publishes one
/// `<node>: activated` line, which goes out because a node's publishers are
/// active by the time its `on_activate` runs.
///
/// Every transition callback prints `<node> on_<transition>`, so a test can
/// see which node's own code ran. `NROS_SPIN_MS` bounds the run; unset, it
/// spins until killed.
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>
#include <rclc/publisher.h>
#include <rclc/timer.h>

#include "std_msgs.h"

#define NODE_COUNT 2

typedef struct {
    const char* name;
    const char* topic;
    rcl_node_t node;
    rcl_publisher_t publisher;
    std_msgs__msg__String message;
    std_msgs__msg__String__Storage storage;
    int count;
} managed_t;

/* Static allocation: every nros struct lives in .bss, not on the stack. */
static struct {
    rclc_support_t support;
    rclc_executor_t executor;
    rcl_timer_t timer;
    managed_t nodes[NODE_COUNT];
} app = {
    .nodes =
        {
            {.name = "lc_alpha", .topic = "/lc_alpha/chatter"},
            {.name = "lc_beta", .topic = "/lc_beta/chatter"},
        },
};

/* Publish `<node>: <text>` on the node's own topic. A node that is not
 * active drops it: the publisher follows the node's state. */
static void say(managed_t* m, const char* text) {
    snprintf(m->message.data.data, m->message.data.capacity, "%s: %s", m->name, text);
    m->message.data.size = strlen(m->message.data.data);
    (void)rcl_publish(&m->publisher, &m->message, NULL);
}

static void timer_callback(struct rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    (void)last_call_time;
    char text[16];
    for (int i = 0; i < NODE_COUNT; ++i) {
        managed_t* m = &app.nodes[i];
        snprintf(text, sizeof text, "%d", ++m->count);
        say(m, text);
    }
}

/* The transition callbacks; `context` is the node's `managed_t`. */
static uint8_t on_configure(void* context) {
    printf("%s on_configure\n", ((managed_t*)context)->name);
    return RCLC_LIFECYCLE_RET_OK;
}

static uint8_t on_activate(void* context) {
    managed_t* m = (managed_t*)context;
    printf("%s on_activate\n", m->name);
    say(m, "activated");
    return RCLC_LIFECYCLE_RET_OK;
}

static uint8_t on_deactivate(void* context) {
    printf("%s on_deactivate\n", ((managed_t*)context)->name);
    return RCLC_LIFECYCLE_RET_OK;
}

static uint8_t on_cleanup(void* context) {
    printf("%s on_cleanup\n", ((managed_t*)context)->name);
    return RCLC_LIFECYCLE_RET_OK;
}

static uint8_t on_shutdown(void* context) {
    printf("%s on_shutdown\n", ((managed_t*)context)->name);
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

/* Bind one node, give it a publisher, and make it a managed node with its
 * own services and its own callbacks. */
static rcl_ret_t make_managed(managed_t* m) {
    /* The namespace is explicit: NULL options leave it EMPTY, and an empty
     * namespace is not the root one. */
    rcl_node_options_t opts = rcl_node_get_default_options();
    opts.namespace_[0] = (uint8_t)'/';
    opts.namespace_len = 1;
    rcl_ret_t ret = rclc_executor_node_init(&app.executor, &m->node, m->name, &opts);
    if (ret != RCL_RET_OK) {
        return ret;
    }
    ret = rclc_publisher_init_default(&m->publisher, &m->node,
                                      ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), m->topic);
    if (ret != RCL_RET_OK) {
        return ret;
    }
    std_msgs__msg__String__init_in(&m->message, &m->storage);

    /* /<node>/get_state, /<node>/change_state, ... and a machine of its own. */
    ret = rclc_node_register_lifecycle_services(&m->node);
    if (ret != RCL_RET_OK) {
        return ret;
    }
    if ((ret = rclc_node_lifecycle_register_on_configure(&m->node, on_configure, m)) !=
            RCL_RET_OK ||
        (ret = rclc_node_lifecycle_register_on_activate(&m->node, on_activate, m)) != RCL_RET_OK ||
        (ret = rclc_node_lifecycle_register_on_deactivate(&m->node, on_deactivate, m)) !=
            RCL_RET_OK ||
        (ret = rclc_node_lifecycle_register_on_cleanup(&m->node, on_cleanup, m)) != RCL_RET_OK ||
        (ret = rclc_node_lifecycle_register_on_shutdown(&m->node, on_shutdown, m)) != RCL_RET_OK) {
        return ret;
    }
    return RCL_RET_OK;
}

static int run(void) {
    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, 0, NULL, &allocator), 1);
    /* 16 handles: five lifecycle services per node, the timer, headroom. */
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 16, NULL), 1);

    for (int i = 0; i < NODE_COUNT; ++i) {
        RCLC_CHECK_RET(make_managed(&app.nodes[i]), 2);
        printf("Lifecycle services registered on /%s (state %u)\n", app.nodes[i].name,
               (unsigned)rclc_node_lifecycle_get_current_state(&app.nodes[i].node));
    }

    /* One timer for both nodes: each tick tries both publishers. */
    RCLC_CHECK_RET(rclc_timer_init_default(&app.timer, &app.support, 200000000ULL, timer_callback),
                   3);
    RCLC_CHECK_RET(rclc_executor_add_timer(&app.executor, &app.timer), 3);

    printf("Ready. Drive them with `ros2 lifecycle set /lc_alpha configure`.\n");

    const unsigned budget = spin_ms();
    for (unsigned waited = 0; budget == 0u || waited < budget; waited += 100) {
        (void)rclc_executor_spin_some(&app.executor, 100ULL * 1000ULL * 1000ULL);
    }

    rcl_timer_fini(&app.timer);
    for (int i = 0; i < NODE_COUNT; ++i) {
        rcl_publisher_fini(&app.nodes[i].publisher, &app.nodes[i].node);
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
