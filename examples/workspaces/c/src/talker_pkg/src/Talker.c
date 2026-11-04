/// @file Talker.c
/// @brief pure-C workspace — the C talker, a typed component (RFC-0043 / phase-257).
///
/// The ROS 2 tutorial talker (demo_nodes_cpp `talker.cpp`): a 1 Hz timer publishes
/// `std_msgs/String` "Hello World: N" on `/chatter`, N counting from 1, and prints
/// `Publishing: '<payload>'`. phase-535 made this the ONE C talker: the standalone example
/// pairs and the workspace images now run the same program (RFC-0122 D1, option B).
///
/// `RCLC_C_COMPONENT` emits the C-ABI factory/configure the generated entry calls; the
/// entry owns the support, node and executor lifecycle, so this file holds only the node.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <std_msgs/std_msgs.h>
#include <rclc/rclc.h>

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    std_msgs__msg__String msg;
    // The bytes behind `msg.data` (phase-492 W7): the component owns them,
    // wired by `std_msgs__msg__String__init_in` below. Nothing allocates.
    std_msgs__msg__String__Storage msg_storage;
    int32_t count;
} talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    talker_t* self = RCLC_CONTAINER_OF(timer, talker_t, timer);
    (void)last_call_time;
    // Pre-increment so the first payload is "Hello World: 1", as the ROS 2 demo talker does.
    self->count++;
    snprintf(self->msg.data.data, self->msg.data.capacity, "Hello World: %d", (int)self->count);
    self->msg.data.size = strlen(self->msg.data.data);
    if (rcl_publish(&self->pub, &self->msg, NULL) == RCL_RET_OK) {
        printf("Publishing: '%s'\n", self->msg.data.data);
    }
    /* Log each tick via the nros-log facade. `rclc_log_default_logger()` is the built-in
     * DEFAULT_LOGGER (level Info; NULL would DROP the record). The first emit lazy-installs the
     * default sink → the platform writer → "[INFO] nros: c_talker logging seq=N". */
    RCLC_LOG_INFO(rclc_log_default_logger(), "c_talker logging seq=%d", (int)self->count);
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor, talker_t* self) {
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    // Line-buffer stdout: glibc full-buffers a pipe, and a test reads this output live.
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif
    self->count = 0;
    std_msgs__msg__String__init_in(&self->msg, &self->msg_storage);
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 1000000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(talker_t, talker_configure)
