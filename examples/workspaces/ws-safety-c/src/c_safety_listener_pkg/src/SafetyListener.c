/// @file SafetyListener.c
/// @brief Phase 269 W3 — C validated-subscription listener for the E2E-safety workspace.
///
/// Registers a validated callback subscription on /chatter via
/// `nros_cpp_subscription_register_validated` (the C component-callback analog of
/// Rust's `create_subscription_for_callback_name_with_safety`). The callback
/// receives the CDR bytes AND the E2E integrity status scalars:
///   - gap       — sequence-number gap since the last in-order message (0 = none)
///   - duplicate — true if the sample was already seen
///   - crc_valid — 1 = CRC ok, 0 = CRC mismatch, -1 = no CRC on the wire
///
/// CRC-valid messages increment the received counter and republish the count on
/// /safe_ok (std_msgs/Int32). The cross-process e2e test (`cpp_c_safety_integrity_e2e.rs`)
/// subscribes to /safe_ok and asserts the count climbs, proving the validated-callback
/// path works end-to-end.
///
/// Requires NANO_ROS_SAFETY_E2E=ON (lowered from `[system].features = ["safety"]`
/// via NanoRosCapabilities.cmake).

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/component.h>
#include <nros/nros_cpp_ffi.h>

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t received;
    int32_t integrity_faults;
} safety_listener_t;

static void write_i32_le(uint8_t* p, int32_t v) {
    uint32_t u = (uint32_t)v;
    p[0] = (uint8_t)u;
    p[1] = (uint8_t)(u >> 8);
    p[2] = (uint8_t)(u >> 16);
    p[3] = (uint8_t)(u >> 24);
}

/// Validated-subscription callback: invoked by the executor on each /chatter
/// sample. The integrity scalars come from `try_recv_validated` in the arena.
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

        /* std_msgs/Int32 CDR: 4-byte encapsulation + int32 */
        uint8_t buf[8];
        buf[0] = 0x00;
        buf[1] = 0x01;
        buf[2] = 0x00;
        buf[3] = 0x00;
        write_i32_le(buf + 4, self->received);
        nros_cpp_publish_raw(self->pub, buf, sizeof(buf));
    } else {
        self->integrity_faults++;
        printf("[LISTENER] integrity fault — crc_valid=%d gap=%lld dup=%s total_faults=%d\n",
               (int)crc_valid, (long long)gap, duplicate ? "true" : "false",
               self->integrity_faults);
        fflush(stdout);
    }
}

static nros_ret_t listener_configure(const nros_cpp_node_t* node, void* executor,
                                     safety_listener_t* self) {
    (void)executor;
    self->received = 0;
    self->integrity_faults = 0;

    /* Create /safe_ok publisher to report CRC-valid counts */
    int32_t rc = nros_cpp_publisher_create(node, "/safe_ok", "std_msgs::msg::dds_::Int32_", "",
                                           nros_c_qos_default(), self->pub);
    if (rc != 0) {
        return rc;
    }

    /* Register the validated subscription on /chatter.
     * The callback receives CRC verdict + sequence info alongside the CDR bytes.
     * Requires NANO_ROS_SAFETY_E2E=ON. */
    size_t handle;
    return nros_cpp_subscription_register_validated(node, "/chatter", "std_msgs::msg::dds_::Int32_",
                                                    "", nros_c_qos_default(), on_chatter_validated,
                                                    self,
                                                    /*sched_context=*/0, &handle);
}

NROS_C_COMPONENT(safety_listener_t, listener_configure)
