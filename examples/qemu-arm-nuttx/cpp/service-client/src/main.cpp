/// @file main.cpp
/// @brief C++ service client example - calls AddTwoInts service (async Future)

#include <stdio.h>
// <stdlib.h> (not <cstdlib>): newlib on the embedded cross toolchains does
// not inject strtoll/getenv into namespace std — the global C spellings are
// the portable ones (this source builds native AND on the RTOS boards).
#include <stdlib.h>

#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/srv/AddTwoInts
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
    printf("nros C++ Service Client (AddTwoInts)\n");
    printf("=====================================\n");

    // Launch-aware init. Falls back to the env overlay
    // (`$NROS_LOCATOR` / `$ROS_DOMAIN_ID` / `$RMW_IMPLEMENTATION`) when
    // no `$NROS_RUNTIME_OVERLAY` / launch XML is in scope.
    NROS_TRY_RET(nros::init_with_launch_auto(argc, argv), 1);

    // Operands from the first two positional args (default: 2 3).
    int64_t a = 2;
    int64_t b = 3;
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
    // Host-only: positional-arg override. Embedded (freestanding C++) has no
    // argv and newlib's freestanding <stdlib.h> declares no strtoll.
    if (argc >= 3) {
        char* end_a = nullptr;
        char* end_b = nullptr;
        long long parsed_a = strtoll(argv[1], &end_a, 10);
        long long parsed_b = strtoll(argv[2], &end_b, 10);
        if (end_a && *end_a == '\0' && end_b && *end_b == '\0') {
            a = static_cast<int64_t>(parsed_a);
            b = static_cast<int64_t>(parsed_b);
        }
    }
#endif

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "add_two_ints_client"), 1);
    printf("Node created: %s\n", node.get_name());

    nros::Client<example_interfaces::srv::AddTwoInts> client;
    NROS_TRY_RET(node.create_client(client, "/add_two_ints"), 1);

    example_interfaces::srv::AddTwoInts::Request req;
    req.a = a;
    req.b = b;

    example_interfaces::srv::AddTwoInts::Response resp;
    auto fut = client.send_request(req);
    if (fut.is_consumed()) {
        fprintf(stderr, "send_request failed\n");
        nros::shutdown();
        return 1;
    }
    nros::Result ret = fut.wait(nros::global_handle(), 5000, resp);

    int exit_code = 0;
    if (ret.ok()) {
        printf("Result of add_two_ints: %lld\n", static_cast<long long>(resp.sum));
    } else {
        fprintf(stderr, "Service call failed with error %d\n", ret.raw());
        exit_code = 1;
    }

    // Cleanup
    printf("\nShutting down...\n");
    nros::shutdown();

    printf("Goodbye!\n");
    return exit_code;
}

NROS_APP_MAIN_REGISTER()
