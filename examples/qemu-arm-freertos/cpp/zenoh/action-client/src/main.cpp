/// @file main.cpp
/// @brief C++ action client — sends Fibonacci goal to /fibonacci (async API, FreeRTOS QEMU)

#include <cstdio>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

static volatile bool g_goal_accepted = false;
static volatile bool g_result_received = false;
static uint8_t g_goal_id[16];

// Forward declaration — needed to call get_result_async from the goal callback
static nros::ActionClient<example_interfaces::action::Fibonacci>* g_client_ptr;

// ----------------------------------------------------------------------------
// Async callbacks (invoked during spin_once)
// ----------------------------------------------------------------------------

static void goal_response_cb(bool accepted, const uint8_t goal_id[16], void* ctx) {
    (void)ctx;
    if (accepted) {
        printf("Goal accepted!\n");
        g_goal_accepted = true;
        // Automatically request the result
        g_client_ptr->get_result_async(goal_id);
    } else {
        printf("Goal rejected!\n");
    }
}

static void feedback_cb(const uint8_t goal_id[16], const uint8_t* data,
                        size_t len, void* ctx) {
    (void)goal_id;
    (void)ctx;

    example_interfaces::action::Fibonacci::Feedback fb;
    if (example_interfaces::action::Fibonacci::Feedback::ffi_deserialize(data, len, &fb) == 0) {
        printf("Feedback: [");
        for (uint32_t i = 0; i < fb.sequence.size; i++) {
            if (i > 0) printf(", ");
            printf("%d", fb.sequence.data[i]);
        }
        printf("]\n");
    }
}

static void result_cb(const uint8_t goal_id[16], int status,
                      const uint8_t* data, size_t len, void* ctx) {
    (void)goal_id;
    (void)status;
    (void)ctx;

    example_interfaces::action::Fibonacci::Result result;
    if (example_interfaces::action::Fibonacci::Result::ffi_deserialize(data, len, &result) == 0) {
        printf("Result: [");
        for (uint32_t i = 0; i < result.sequence.size; i++) {
            if (i > 0) printf(", ");
            printf("%d", result.sequence.data[i]);
        }
        printf("]\n");
    }

    printf("\nAction completed successfully.\n");
    g_result_received = true;
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

extern "C" void app_main(void) {
    printf("nros C++ Action Client (FreeRTOS) [async]\n");
    nros::Result ret = nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_action_client");
    if (!ret.ok()) { printf("create_node failed\n"); nros::shutdown(); return; }
    printf("Node created\n");

    nros::ActionClient<example_interfaces::action::Fibonacci> client;
    ret = node.create_action_client(client, "/fibonacci");
    if (!ret.ok()) { printf("create_action_client failed: %d\n", ret.raw()); nros::shutdown(); return; }
    g_client_ptr = &client;

    printf("Action client ready for /fibonacci\n");

    // Warm-up: spin to allow Zenoh to discover the server's queryables
    for (int i = 0; i < 500; i++) {
        nros::spin_once(10);
    }

    example_interfaces::action::Fibonacci::Goal goal;
    goal.order = 5;

    printf("Sending goal: order=%d\n", goal.order);

    // Send goal asynchronously — returns immediately.
    // Responses arrive via callbacks during spin_once.
    ret = client.send_goal_async(goal, g_goal_id);
    if (!ret.ok()) {
        printf("Failed to send goal: %d\n", ret.raw());
        nros::shutdown();
        return;
    }

    // Spin until result received or timeout (30s = 3000 × 10ms)
    for (int i = 0; i < 3000 && !g_result_received; i++) {
        nros::spin_once(10);

        // Poll feedback manually (try_recv_feedback is non-blocking)
        example_interfaces::action::Fibonacci::Feedback fb;
        if (client.try_recv_feedback(fb)) {
            printf("Feedback: [");
            for (uint32_t j = 0; j < fb.sequence.size; j++) {
                if (j > 0) printf(", ");
                printf("%d", fb.sequence.data[j]);
            }
            printf("]\n");
        }
    }

    if (!g_result_received) {
        printf("Timeout waiting for result\n");
    }

    nros::shutdown();
}
