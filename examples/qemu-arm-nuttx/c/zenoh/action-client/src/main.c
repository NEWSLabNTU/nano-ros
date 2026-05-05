/// @file main.c
/// @brief NuttX C action client example - sends Fibonacci goal, gets result

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include <nros/action.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>

#include "example_interfaces.h"

// NuttX embedded config — matches board crate defaults (client = 192.0.3.11)
#ifndef APP_ZENOH_LOCATOR
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#endif
#ifndef APP_DOMAIN_ID
#define APP_DOMAIN_ID 0
#endif

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_action_client_t action_client;
    nros_executor_t executor;
} app;

void app_main(void) {

    printf("nros NuttX C Action Client (Fibonacci)\n");
    printf("Locator: %s\n", APP_ZENOH_LOCATOR);

    memset(&app, 0, sizeof(app));

    // Re-seed /dev/urandom with a per-example unique value. NuttX's
    // xorshift128 PRNG starts with a fixed seed, so two QEMU instances
    // otherwise generate identical Zenoh session IDs and zenohd rejects
    // the second connection with MAX_LINKS. Writing bytes to /dev/urandom
    // reseeds the PRNG state.
    {
        FILE* urandom = fopen("/dev/urandom", "wb");
        if (urandom != NULL) {
            const uint8_t seed[4] = {10, 0, 2, 35};
            fwrite(seed, 1, sizeof(seed), urandom);
            fclose(urandom);
        }
    }

    nros_action_type_t fibonacci_type = {
        .type_name = example_interfaces_action_fibonacci_get_type_name(),
        .type_hash = example_interfaces_action_fibonacci_get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    // Wait for NuttX networking to become ready before attempting the
    // zenoh TCP session. NuttX's poll()/select() don't cooperate with
    // blocking connect() well enough to rely on connect_timeout, so we
    // just sleep for a few seconds after boot and let the virtio-net
    // driver + DHCP/static IP setup finish. Mirrors the 5-second wait
    // in packages/boards/nros-board-nuttx-qemu-arm/src/node.rs::run().
    fflush(stdout);
    sleep(5);

    NROS_CHECK(nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID));
    NROS_CHECK(nros_node_init(&app.node, &app.support, "nuttx_c_action_client", "/"));
    NROS_CHECK(nros_action_client_init(
        &app.action_client, &app.node, "/fibonacci", &fibonacci_type));
    NROS_CHECK(nros_executor_init(&app.executor, &app.support, 4));
    NROS_CHECK(nros_executor_add_action_client(&app.executor, &app.action_client));

    // Race 3 fix (Phase 89.13): probe the action server's send_goal
    // queryable liveliness token before the first send_goal so we
    // don't race the queryable's declare-ack from the router on cold
    // boot. See the service-client example for the full rationale.
    NROS_CHECK(nros_action_client_wait_for_action_server(&app.action_client, &app.executor, 10000));
    printf("Action server discovered — sending goal\n");
    nros_ret_t ret = NROS_RET_OK;
    fflush(stdout);

    example_interfaces_action_fibonacci_goal goal;
    example_interfaces_action_fibonacci_goal_init(&goal);
    goal.order = 10;

    uint8_t goal_buf[64];
    int32_t goal_len = example_interfaces_action_fibonacci_goal_serialize(
        &goal, goal_buf, sizeof(goal_buf));
    if (goal_len < 0) {
        fprintf(stderr, "Failed to serialize goal\n");
        goto cleanup;
    }

    printf("Sending goal: order=%d\n", goal.order);
    // NuttX libc full-buffers stdout when the output is a pipe (as under
    // QEMU serial capture). Flush before each blocking call so the test
    // harness sees progress while the call spins. See the similar fflushes
    // already present in the talker example.
    fflush(stdout);

    nros_goal_uuid_t goal_uuid;
    ret = nros_action_send_goal(
        &app.action_client, &app.executor, goal_buf, (size_t)goal_len, &goal_uuid);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to send goal: %d\n", ret);
        goto cleanup;
    }

    printf("Goal accepted! (uuid=%02x%02x%02x%02x...)\n",
           goal_uuid.uuid[0], goal_uuid.uuid[1],
           goal_uuid.uuid[2], goal_uuid.uuid[3]);

    printf("Waiting for result...\n\n");
    fflush(stdout);

    nros_goal_status_t final_status;
    uint8_t result_buf[512];
    size_t result_len = 0;
    ret = nros_action_get_result(
        &app.action_client, &app.executor, &goal_uuid, &final_status,
        result_buf, sizeof(result_buf), &result_len);

    if (ret == NROS_RET_OK) {
        printf("Result (status=%s): ",
               nros_goal_status_to_string(final_status));

        example_interfaces_action_fibonacci_result result;
        if (example_interfaces_action_fibonacci_result_deserialize(
                &result, result_buf, result_len) == 0) {
            printf("[");
            for (uint32_t i = 0; i < result.sequence.size; i++) {
                if (i > 0) printf(", ");
                printf("%d", result.sequence.data[i]);
            }
            printf("]\n");
        } else {
            printf("(deserialize failed)\n");
        }
        // Marker that `QemuProcess::wait_for_output` early-exits on,
        // matching every other action-client example (FreeRTOS C/C++,
        // NuttX C++, ThreadX C/C++). Without this the test harness
        // sits at the 240s NuttX-C action timeout because NuttX's
        // flat-build kernel never lets the QEMU process exit after
        // app_main returns.
        printf("Action completed successfully.\n");
    } else if (ret == NROS_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for result\n");
    } else {
        fprintf(stderr, "Failed to get result: %d\n", ret);
    }
    fflush(stdout);

cleanup:
    nros_executor_fini(&app.executor);
    nros_action_client_fini(&app.action_client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
    return (ret == NROS_RET_OK) ? 0 : 1;
}
