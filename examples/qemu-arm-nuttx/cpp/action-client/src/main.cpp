/// @file main.cpp
/// @brief C++ action client — sends Fibonacci goal to /fibonacci (NuttX QEMU, async API)
// Uses the callback-based async API. For the Future-based alternative,
// see the native/cpp/action-client example.

#include <cstdint>
#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>
#include <nros/app_config.h>
#include "example_interfaces.hpp"

using Fibonacci = example_interfaces::action::Fibonacci;

static volatile bool g_result_received = false;
static volatile bool g_goal_accepted = false;
static nros::ActionClient<Fibonacci>* g_client_ptr;

static void goal_response_cb(bool accepted, const uint8_t goal_id[16], void* ctx) {
    (void)ctx;
    if (accepted) {
        printf("Goal accepted!\n");
        fflush(stdout);
        g_goal_accepted = true;
        g_client_ptr->get_result_async(goal_id);
    } else {
        printf("Goal rejected!\n");
        fflush(stdout);
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

static void result_cb(const uint8_t goal_id[16], int32_t status,
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
    fflush(stdout);
    g_result_received = true;
}

#ifndef __NuttX__
extern "C" int sleep(unsigned int);
#endif
int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C++ Action Client (NuttX) [async]\n");

    // Re-seed /dev/urandom (see talker for rationale). Unique seed per example.
    if (FILE* urandom = fopen("/dev/urandom", "wb")) {
        const uint8_t seed[4] = {10, 0, 2, 45};
        fwrite(seed, 1, sizeof(seed), urandom);
        fclose(urandom);
    }

    // Wait for NuttX networking to come up (mirrors the C examples).
    sleep(5);
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
        client.poll();
    }

    Fibonacci::Goal goal;
    goal.order = 5;

    printf("Sending goal: order=%d\n", goal.order);
    fflush(stdout);

    uint8_t goal_id[16];

    // Phase 177.30 — DO NOT shrink this ceiling, and keep the resend.
    //
    // The full goal→accept→feedback→result chain is very slow on NuttX QEMU
    // under `-icount` + heavy `test-all` host load (two QEMU guests + zenohd
    // all competing): each `spin_once(10)` can cost >200 ms of wall time, so
    // the old 1000-iteration cap gave up (printing "Timeout waiting for
    // result", accepted=false) BEFORE the goal was even accepted — which
    // looked like a hang/deadlock but is plain slowness (a direct,
    // lightly-loaded boot completes: accepted + feedback + result
    // [0,1,1,2,3,5]). Two robustness measures, both needed:
    //   1. High ceiling — the loop breaks immediately on completion, so this
    //      only matters when the host is slow; the real wall-clock bound is
    //      the harness's `client_timeout` (rtos_e2e.rs — also DO NOT shrink).
    //   2. Resend until accepted — `send_goal_async` is a one-shot zenoh
    //      query; on a cold NuttX boot it can fire before the server's
    //      queryable is discovered and be silently dropped. The blocking C
    //      action client survives because its `send_goal` spins-and-retries
    //      internally; this async example must resend explicitly.
    for (int i = 0; i < 60000 && !g_result_received; i++) {
        if (!g_goal_accepted && (i % 300) == 0) {
            ret = client.send_goal_async(goal, goal_id);
            if (!ret.ok()) {
                printf("send_goal_async failed: %d (will retry)\n", ret.raw());
                fflush(stdout);
            }
        }
        nros::spin_once(10);
        client.poll();
    }

    if (!g_result_received) {
        printf("Timeout waiting for result\n");
        fflush(stdout);
    }

    nros::shutdown();
    return 0;
}

/* Phase 157 — NuttX external-app build (canonical
 * apps/external/<name>/) defines NROS_NUTTX_EXTERNAL_APP=1 via the
 * sibling Makefile so the auto-detect macro picks the
 * `int main(int, char**)` entry that NuttX's Application.mk
 * renames to `<PROGNAME>_main`. QEMU cmake bring-up (Phase 144.6)
 * leaves the define unset and stays on `app_main(void)` for the
 * nros-board-nuttx-qemu-arm Rust shim. */
NROS_APP_MAIN_REGISTER()
