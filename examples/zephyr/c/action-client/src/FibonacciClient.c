/// @file FibonacciClient.c
/// @brief Zephyr C Fibonacci action client — typed component.
///
/// `client_configure` creates an action client with the C API, registers the
/// goal-response / feedback / result callbacks, a poll timer that drains the
/// replies each tick, then sends one goal (`rclc_action_send_goal_async`).
/// Acceptance and the result arrive in the callbacks.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>

#include <example_interfaces/action/fibonacci.h>

typedef struct {
    rcl_action_client_t client;
    rcl_timer_t timer;
    rclc_goal_uuid_t goal_id;
    int32_t order;
} fib_client_t;

/* Print an int32 sequence CDR payload as `<prefix>[0, 1, 1, ...]`. */
static void print_sequence(const char* prefix, const uint8_t* data, size_t len) {
    example_interfaces__action__Fibonacci_Result r;
    example_interfaces__action__Fibonacci_Result__Storage storage;
    example_interfaces__action__Fibonacci_Result__init_in(&r, &storage);
    printf("%s[", prefix);
    if (example_interfaces__action__Fibonacci_Result__deserialize(&r, data, len) == 0) {
        size_t i;
        for (i = 0; i < r.sequence.size; ++i) {
            if (i > 0) {
                printf(", ");
            }
            printf("%d", (int)r.sequence.data[i]);
        }
    }
    printf("]\n");
}

static void on_goal_response(const rclc_goal_uuid_t* goal_id, bool accepted, void* ctx) {
    fib_client_t* self = (fib_client_t*)ctx;
    if (accepted) {
        printf("Goal accepted by server, waiting for result\n");
        (void)rclc_action_get_result_async(&self->client, goal_id);
    } else {
        printf("Goal rejected by server\n");
    }
}

static void on_feedback(const rclc_goal_uuid_t* goal_id, const uint8_t* data, size_t len,
                        void* ctx) {
    (void)goal_id;
    (void)ctx;
    /* Feedback and Result carry the same `int32[] sequence` field. */
    print_sequence("Next number in sequence received: ", data, len);
}

static void on_result(const rclc_goal_uuid_t* goal_id, rclc_goal_status_t status,
                      const uint8_t* data, size_t len, void* ctx) {
    (void)goal_id;
    (void)status;
    (void)ctx;
    print_sequence("Result received: ", data, len);
}

static void on_poll(rcl_timer_t* timer, void* ctx) {
    (void)timer;
    fib_client_t* self = (fib_client_t*)ctx;
    (void)rclc_action_client_poll(&self->client); /* drain replies -> callbacks */
}

static rcl_ret_t client_configure(rcl_node_t* node, rclc_executor_t* executor, fib_client_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0); /* callbacks print on transitions only */
    self->order = 10;
    self->client = rcl_action_get_zero_initialized_client();
    rcl_ret_t rc = rcl_action_client_init(
        &self->client, node, ROSIDL_GET_ACTION_TYPE_SUPPORT(example_interfaces, action, Fibonacci),
        "/fibonacci", NULL);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    (void)rclc_action_client_set_goal_response_callback(&self->client, on_goal_response, self);
    (void)rclc_action_client_set_feedback_callback(&self->client, on_feedback, self);
    (void)rclc_action_client_set_result_callback(&self->client, on_result, self);
    rc = rclc_executor_attach_action_client(executor, &self->client);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_with_context(&self->timer, 20ULL * 1000000ULL, on_poll, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_timer(executor, &self->timer);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    /* Send one goal (async — acceptance arrives in on_goal_response). */
    example_interfaces__action__Fibonacci_Goal goal = {.order = self->order};
    uint8_t buf[16];
    size_t n = 0;
    printf("Sending goal\n");
    if (example_interfaces__action__Fibonacci_Goal__serialize(&goal, buf, sizeof(buf), &n) == 0) {
        (void)rclc_action_send_goal_async(&self->client, buf, n, &self->goal_id);
    }
    return RCL_RET_OK;
}

RCLC_C_COMPONENT(fib_client_t, client_configure)
