/// @file main.cpp
/// @brief C++ action client — sends Fibonacci goal to /fibonacci (ThreadX RISC-V QEMU, async API)
// Uses the callback-based async API. For the Future-based alternative,
// see the native/cpp/zenoh/action-client example.

#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>
#include <nros/app_config.h>
#include "example_interfaces.hpp"

using Fibonacci = example_interfaces::action::Fibonacci;

static volatile bool g_result_received = false;
static nros::ActionClient<Fibonacci>* g_client_ptr;

static void goal_response_cb(bool accepted, const uint8_t goal_id[16], void* ctx) {
    (void)ctx;
    if (accepted) {
        printf("Goal accepted!\n");
        g_client_ptr->get_result_async(goal_id);
    } else {
        printf("Goal rejected!\n");
    }
}

static void feedback_cb(const uint8_t goal_id[16], const uint8_t* data,
                        size_t len, void* ctx) {
    (void)goal_id;
    (void)ctx;

    Fibonacci::Feedback fb;
    if (Fibonacci::Feedback::ffi_deserialize(data, len, &fb) == 0) {
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

    Fibonacci::Result result;
    if (Fibonacci::Result::ffi_deserialize(data, len, &result) == 0) {
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

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C++ Action Client (ThreadX RISC-V) [async]\n");
    NROS_TRY_RET(nros::init(NROS_APP_CONFIG.zenoh.locator, NROS_APP_CONFIG.zenoh.domain_id), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_action_client"), 1);
    printf("Node created\n");

    nros::ActionClient<Fibonacci> client;
    NROS_TRY_RET(node.create_action_client(client, "/fibonacci"), 1);
    g_client_ptr = &client;
    nros::Result ret;

    nros::ActionClient<Fibonacci>::SendGoalOptions opts;
    opts.goal_response = goal_response_cb;
    opts.feedback = feedback_cb;
    opts.result = result_cb;
    client.set_callbacks(opts);

    printf("Action client ready for /fibonacci\n");

    // Warm-up: spin to allow Zenoh to discover the server's queryables
    for (int i = 0; i < 500; i++) {
        nros::spin_once(10);
    }

    Fibonacci::Goal goal;
    goal.order = 5;

    printf("Sending goal: order=%d\n", goal.order);

    uint8_t goal_id[16];
    ret = client.send_goal_async(goal, goal_id);
    if (!ret.ok()) {
        printf("Failed to send goal: %d\n", ret.raw());
        nros::shutdown();
        return;
    }

    for (int i = 0; i < 1000 && !g_result_received; i++) {
        nros::spin_once(10);
    }

    if (!g_result_received) {
        printf("Timeout waiting for result\n");
    }

    nros::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER_VOID()
