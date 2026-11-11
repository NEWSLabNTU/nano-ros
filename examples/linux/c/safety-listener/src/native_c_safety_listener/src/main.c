/// @file main.c
/// @brief C E2E-safety listener — polls `rclc_polling_subscription_take_validated`
///        and prints the per-message integrity status (CRC + sequence gap/dup).
///
/// Issue 0073 — the C analog of the Rust declarative-safety-listener. Pairs with
/// the safety talker (the imperative Rust example built `--features safety-e2e`):
/// receives `std_msgs/Int32` on `/chatter` over zenohd and validates the CRC
/// attached by the publisher. Built with `NROS_SAFETY_E2E=ON` (set below) so
/// the zenoh backend carries the CRC path.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <rclc/init.h>
#include <rclc/node.h>
#include <rclc/subscription.h>

#include "std_msgs.h"

static struct {
    rclc_support_t support;
    rcl_node_t node;
    rclc_polling_subscription_t subscription;
} app;

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    printf("nros C Safety Listener\n");
    memset(&app, 0, sizeof(app));
    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "c_safety_listener", "/", &app.support), 1);

    // Polling-mode subscription — the validated receive path is poll-only.
    RCLC_CHECK_RET(rclc_polling_subscription_init(&app.subscription, &app.node,
                                                  ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32),
                                                  "/chatter"),
                   1);
    printf("Waiting for Int32 messages on /chatter...\n");

    uint8_t buf[256];
    int count = 0;
    for (;;) {
        rclc_integrity_status_t status;
        int32_t n =
            rclc_polling_subscription_take_validated(&app.subscription, buf, sizeof(buf), &status);
        if (n > 0) {
            std_msgs__msg__Int32 msg;
            memset(&msg, 0, sizeof(msg));
            if (std_msgs__msg__Int32__deserialize(&msg, buf, (size_t)n) == 0) {
                count++;
                const char* crc = status.crc_valid == 1   ? "ok"
                                  : status.crc_valid == 0 ? "FAIL"
                                                          : "n-a";
                printf("[%d] Received: data=%d [SAFETY] INTEGRITY gap=%lld dup=%d crc=%s\n", count,
                       msg.data, (long long)status.gap, (int)status.duplicate, crc);
            }
        } else if (n == 0) {
            usleep(5000);
        } else {
            fprintf(stderr, "recv error: %d\n", n);
            usleep(5000);
        }
    }

    rclc_polling_subscription_fini(&app.subscription);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}
