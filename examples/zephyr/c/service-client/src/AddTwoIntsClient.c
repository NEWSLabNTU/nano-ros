/// @file AddTwoIntsClient.c
/// @brief Zephyr C AddTwoInts service client — typed component.
///
/// `client_configure` creates a service client + a timer that polls: the first
/// tick sends ONE fixed request (2, 3); later ticks poll the reply and print
/// the sum, then the client goes quiet. Poll model, with the generated typed
/// helpers (`example_interfaces__srv__AddTwoInts__client_{send_request,
/// take_response}`) over the C API.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>

#include <example_interfaces/srv/add_two_ints.h>

typedef struct {
    rcl_client_t client;
    rcl_timer_t timer;
    int64_t a;
    int64_t b;
    int awaiting;
    int done;
} add_client_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    add_client_t* self = RCLC_CONTAINER_OF(timer, add_client_t, timer);
    (void)last_call_time;
    uint8_t scratch[64];
    if (self->done) {
        return;
    }
    if (!self->awaiting) {
        example_interfaces__srv__AddTwoInts_Request req = {.a = self->a, .b = self->b};
        if (example_interfaces__srv__AddTwoInts__client_send_request(
                &self->client, &req, scratch, sizeof(scratch)) == RCL_RET_OK) {
            self->awaiting = 1;
        }
        return;
    }
    example_interfaces__srv__AddTwoInts_Response resp;
    if (example_interfaces__srv__AddTwoInts__client_take_response(&self->client, &resp, scratch,
                                                                  sizeof(scratch)) == RCL_RET_OK) {
        printf("Result of add_two_ints: %lld\n", (long long)resp.sum);
        self->done = 1;
    }
}

static rcl_ret_t client_configure(rcl_node_t* node, rclc_executor_t* executor, add_client_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    self->a = 2;
    self->b = 3;
    self->awaiting = 0;
    self->done = 0;
    self->client = rcl_get_zero_initialized_client();
    rcl_ret_t rc = rclc_client_init_default(
        &self->client, node, ROSIDL_GET_SRV_TYPE_SUPPORT(example_interfaces, srv, AddTwoInts),
        "/add_two_ints");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_attach_client(executor, &self->client);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 1000ULL * 1000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_timer(executor, &self->timer);
    if (rc == RCL_RET_OK) {
        printf("Sending request\n");
    }
    return rc;
}

RCLC_C_COMPONENT(add_client_t, client_configure)
