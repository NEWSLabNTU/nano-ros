/// @file SafetyListener.c
/// @brief C validated-subscription listener for the E2E-safety workspace.
///
/// Registers a validated callback subscription on /chatter via
/// `rclc_executor_add_subscription_raw_validated` (the C API's analog of
/// Rust's `create_subscription_for_callback_name_with_safety`). The callback
/// receives the CDR bytes AND the E2E integrity status scalars:
///   - gap       — sequence-number gap since the last in-order message (0 = none)
///   - duplicate — true if the sample was already seen
///   - crc_valid — 1 = CRC ok, 0 = CRC mismatch, -1 = no CRC on the wire
///
/// CRC-valid messages increment the received counter and republish the count on
/// /safe_ok (std_msgs/Int32, generated C serializer). The cross-process e2e test
/// (`cpp_c_safety_integrity_e2e.rs`) subscribes to /safe_ok and asserts the count
/// climbs, proving the validated-callback path works end-to-end.
///
/// Requires NROS_SAFETY_E2E=ON (lowered from `[system].features = ["safety"]`
/// via NanoRosCapabilities.cmake).

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

#include "std_msgs.h"

typedef struct {
    rcl_subscription_t sub;
    rcl_publisher_t pub;
    int32_t received;
    int32_t integrity_faults;
} safety_listener_t;

/// Validated-subscription callback: invoked by the executor on each /chatter
/// sample. The integrity scalars come from `take_validated` in the arena.
static void on_chatter_validated(const uint8_t* data, size_t len, int64_t gap, bool duplicate,
                                 int8_t crc_valid, void* ctx) {
    (void)data;
    (void)len;
    safety_listener_t* self = (safety_listener_t*)ctx;

    if (crc_valid == 1) {
        /* CRC ok — increment count and republish on /safe_ok */
        self->received++;
        printf("[LISTENER] CRC ok — count=%d gap=%lld dup=%s\n", self->received, (long long)gap,
               duplicate ? "true" : "false");
        fflush(stdout);

        std_msgs__msg__Int32 msg = {.data = self->received};
        (void)rcl_publish(&self->pub, &msg, NULL);
    } else {
        self->integrity_faults++;
        printf("[LISTENER] integrity fault — crc_valid=%d gap=%lld dup=%s total_faults=%d\n",
               (int)crc_valid, (long long)gap, duplicate ? "true" : "false",
               self->integrity_faults);
        fflush(stdout);
    }
}

static rcl_ret_t listener_configure(rcl_node_t* node, rclc_executor_t* executor,
                                    safety_listener_t* self) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->received = 0;
    self->integrity_faults = 0;

    /* Create /safe_ok publisher to report CRC-valid counts */
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/safe_ok");
    if (rc != RCL_RET_OK) {
        return rc;
    }

    /* Register the validated subscription on /chatter.
     * The callback receives CRC verdict + sequence info alongside the CDR bytes.
     * Requires NROS_SAFETY_E2E=ON. */
    self->sub = rcl_get_zero_initialized_subscription();
    rc = rclc_subscription_init_default(
        &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_subscription_raw_validated(executor, &self->sub, on_chatter_validated,
                                                        self, RCLC_EXECUTOR_ON_NEW_DATA);
}

RCLC_C_COMPONENT(safety_listener_t, listener_configure)
