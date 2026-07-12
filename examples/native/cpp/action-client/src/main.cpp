/// @file main.cpp
/// @brief C++ action client example - Fibonacci (blocking)

#include <stdio.h>
// <stdlib.h> (not <cstdlib>): newlib on the embedded cross toolchains does
// not inject strtoll/getenv into namespace std — the global C spellings are
// the portable ones (this source builds native AND on the RTOS boards).
#include <stdlib.h>

#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/action/Fibonacci
#include "example_interfaces.hpp"

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char** argv) {
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, nullptr, _IOLBF, 0);
#endif
    printf("nros C++ Action Client (Fibonacci)\n");
    printf("===================================\n");

    // Launch-aware init. Env overlay active today.
    NROS_TRY_RET(nros::init_with_launch_auto(argc, argv), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "fibonacci_action_client"), 1);
    printf("Node created: %s\n", node.get_name());

    nros::ActionClient<example_interfaces::action::Fibonacci> client;
    NROS_TRY_RET(node.create_action_client(client, "/fibonacci"), 1);
    nros::Result ret;

    // Default order=10; override via NROS_TEST_GOAL_ORDER for tests that
    // want to exercise server-side rejection (order >= 64) or other edges.
    int32_t order = 10;
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
    // Host-only: env override. Embedded (freestanding C++) has no environment
    // and newlib's freestanding <stdlib.h> declares no getenv/atoi.
    if (const char* ord = getenv("NROS_TEST_GOAL_ORDER")) {
        order = atoi(ord);
    }
#endif
    printf("\nSending goal\n");

    example_interfaces::action::Fibonacci::Goal goal;
    goal.order = order;

    uint8_t goal_id[16];
    ret = client.send_goal(goal, goal_id);
    if (!ret.ok()) {
        fprintf(stderr, "Goal was rejected by server (order=%d, ret=%d)\n", order, ret.raw());
        nros::shutdown();
        return 2;
    }
    printf("Goal accepted by server, waiting for result\n");

    // Poll for feedback while waiting — drain via the Stream<T> API,
    // which aligns the feedback receive surface with
    // Subscription<M>::stream() / Promise<T>::wait(). `try_recv_feedback`
    // below is still supported for callers that want the bool-convertible
    // helper.
    auto& feedback = client.feedback_stream();
    for (int i = 0; i < 20; i++) {
        nros::spin_once(100);

        example_interfaces::action::Fibonacci::Feedback fb;
        while (feedback.try_next(fb).ok()) {
            printf("Next number in sequence received: [");
            for (uint32_t k = 0; k < fb.sequence.length(); k++) {
                if (k > 0) printf(", ");
                printf("%d", fb.sequence[k]);
            }
            printf("]\n");
        }
    }

    // Get result (blocking)
    example_interfaces::action::Fibonacci::Result result;
    ret = client.get_result(goal_id, result);
    if (ret.ok()) {
        printf("Result received: [");
        for (uint32_t i = 0; i < result.sequence.length(); i++) {
            if (i > 0) printf(", ");
            printf("%d", result.sequence[i]);
        }
        printf("]\n");
    } else {
        fprintf(stderr, "Failed to get result: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }

    // Cleanup
    printf("\nShutting down...\n");
    nros::shutdown();

    printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER()
