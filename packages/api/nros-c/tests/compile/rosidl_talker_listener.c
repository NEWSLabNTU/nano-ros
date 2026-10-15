/*
 * phase-492 W8 (RFC-0089 "`ROSIDL_GET_*_TYPE_SUPPORT` adopted as a
 * pass-through") — a micro-ROS-style talker and listener.
 *
 * Every line that touches a MESSAGE is the line micro-ROS writes:
 * `ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String)` passed straight to
 * `rclc_publisher_init_default` / `rclc_subscription_init_default`,
 * `rcl_publish(&publisher, &msg, NULL)`, and
 * `rclc_executor_add_subscription_with_context` with rclc's six arguments and
 * a callback that casts `msgin` and reads FIELDS, invoked `ON_NEW_DATA` (rclc's
 * own enumerator).
 *
 * The descriptor is also an ADDRESS CONSTANT here, so it works in a static
 * initialiser — a superset of upstream's function-call macro.
 *
 * Compile-only (no main), `-Wall -Wextra -Werror`.
 */

#include <rcl/rcl.h>
#include <rclc/executor.h>
#include <rclc/rclc.h>
#include <std_msgs/msg/string.h>

#include <stdio.h>

static const rclc_message_type_t* const kStringTypeSupport =
    ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String);

static rclc_support_t support;
static rcl_node_t node;
static rcl_publisher_t publisher;
static rcl_subscription_t subscriber;
static rclc_executor_t executor;

static std_msgs__msg__String outgoing;
static std_msgs__msg__String incoming;
static char outgoing_buffer[64];
static char incoming_buffer[64];

static void subscription_callback(const void* msgin, void* context) {
    const std_msgs__msg__String* msg = (const std_msgs__msg__String*)msgin;
    (void)context;
    printf("I heard: [%.*s]\n", (int)msg->data.size, msg->data.data);
}

int talker_listener(void);
int talker_listener(void) {
    rcl_ret_t rc = rclc_support_init_with_locator(&support, "tcp/127.0.0.1:7447", 0);
    if (rc != RCL_RET_OK) return 1;
    rc = rclc_node_init_default(&node, "micro_ros_node", "", &support);
    if (rc != RCL_RET_OK) return 1;

    rc = rclc_publisher_init_default(&publisher, &node,
                                     ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "chatter");
    if (rc != RCL_RET_OK) return 1;
    rc = rclc_subscription_init_default(&subscriber, &node, kStringTypeSupport, "chatter");
    if (rc != RCL_RET_OK) return 1;

    outgoing.data.data = outgoing_buffer;
    outgoing.data.capacity = sizeof(outgoing_buffer);
    outgoing.data.size =
        (size_t)snprintf(outgoing.data.data, outgoing.data.capacity, "Hello World: %d", 1);

    incoming.data.data = incoming_buffer;
    incoming.data.size = 0;
    incoming.data.capacity = sizeof(incoming_buffer);

    rc = rclc_executor_init(&executor, &support.context, 1, NULL);
    if (rc != RCL_RET_OK) return 1;
    rc = rclc_executor_add_subscription_with_context(&executor, &subscriber, &incoming,
                                                     &subscription_callback, NULL, ON_NEW_DATA);
    if (rc != RCL_RET_OK) return 1;

    rc = rcl_publish(&publisher, &outgoing, NULL);
    if (rc != RCL_RET_OK) return 1;
    return rclc_executor_spin_some(&executor, 100000000ULL) == RCL_RET_OK ? 0 : 1;
}

/* The take side of a subscription the caller drains: the `rcl_take` analog
 * (`rcl_take` itself takes an executor-owned `rcl_subscription_t` here, which
 * has no subscriber of its own to take from — see `<rclc/subscription.h>`). */
int take_one(rclc_polling_subscription_t* sub);
int take_one(rclc_polling_subscription_t* sub) {
    return rclc_polling_subscription_take(sub, &incoming, NULL) == RCL_RET_OK ? 0 : 1;
}
