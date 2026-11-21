/// @file main.c
/// @brief Issue 2805 -- an owned entry whose one publisher carries the
/// workspace's own `owned_census_msgs/msg/Tally`. Its census (a host build
/// of this package alone) has to generate that package to compile this file.

#include <signal.h>
#include <stdio.h>
#include <string.h>

#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>
#include <rclc/publisher.h>
#include <rclc/timer.h>

// The workspace-local message package, generated from src/owned_census_msgs.
#include "owned_census_msgs.h"

static struct {
    rclc_support_t support;
    rcl_node_t node;
    rcl_publisher_t publisher;
    owned_census_msgs__msg__Tally message;
    rcl_timer_t timer;
    rclc_executor_t executor;
} app;

static rclc_executor_t* g_executor = NULL;

static void signal_handler(int signum) {
    (void)signum;
    if (g_executor) {
        rclc_executor_cancel(g_executor);
    }
}

static void timer_callback(struct rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    (void)last_call_time;
    app.message.count++;
    RCLC_SOFTCHECK(rcl_publish(&app.publisher, &app.message, NULL));
    printf("Publishing: %d\n", (int)app.message.count);
}

int main(int argc, char** argv) {
    memset(&app, 0, sizeof(app));

    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 4, NULL), 1);
    g_executor = &app.executor;
    RCLC_CHECK_RET(rclc_executor_node_init(&app.executor, &app.node, "tally_talker", NULL), 1);
    RCLC_CHECK_RET(rclc_publisher_init_default(&app.publisher, &app.node,
                                               ROSIDL_GET_MSG_TYPE_SUPPORT(owned_census_msgs,
                                                                           msg, Tally),
                                               "/tally"),
                   1);
    RCLC_CHECK_RET(rclc_timer_init_default(&app.timer, &app.support, 1000000000ULL, timer_callback),
                   1);
    RCLC_CHECK_RET(rclc_executor_add_timer(&app.executor, &app.timer), 1);

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    rcl_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != RCL_RET_OK) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    rcl_timer_fini(&app.timer);
    rcl_publisher_fini(&app.publisher, &app.node);
    rcl_node_fini(&app.node);
    rclc_executor_fini(&app.executor);
    rclc_support_fini(&app.support);
    return 0;
}
