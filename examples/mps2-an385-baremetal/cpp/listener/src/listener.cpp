/// @file listener.cpp
/// @brief C++ listener for the PURE BARE-METAL MPS2-AN385 — receives
///        `std_msgs/String` on `/chatter` and logs each one.
///
/// The application is ordinary nano-ros C++, the same program as
/// `examples/mps2-an385-freertos/cpp/listener/src/main.cpp`. What differs is a
/// property of having no RTOS, not of the C++ API (issue 1512) — see the
/// `talker` sibling's header: no libc or C++ runtime (`-ffreestanding`), and no
/// `printf`, so log lines are built by hand and emitted through
/// `nros_log_emit_at`.
///
/// `NROS_APP_MAIN_REGISTER()` emits `void app_main(void)` on this platform and
/// `src/main.rs` calls it once the board is up.

#include <stddef.h>
#include <stdint.h>
// `strlen` only — one of the freestanding symbols `nros-baremetal-common`'s
// `libc-stubs` defines.
#include <string.h>

#include <nros/app_main.h>
#include <nros/log.h>
#include <nros/nros.hpp>

// Generated C++ bindings (`nros generate cpp`, run by build.rs).
#include "std_msgs.hpp"

// Backend registration — the image's, as on the cmake road (see the talker
// sibling): `nros_cpp_init` calls it, and bare metal walks no `.init_array`.
extern "C" int nros_rmw_zenoh_register(void);

extern "C" void nros_app_register_backends(void) {
    (void)nros_rmw_zenoh_register();
}

namespace {

size_t append_str(char* out, size_t cap, size_t pos, const char* text) {
    while (*text != '\0' && pos + 1u < cap) {
        out[pos++] = *text++;
    }
    return pos;
}

void emit(nros_logger_t logger, nros_log_severity_t severity, const char* text, const char* file,
          uint32_t line) {
    nros_log_emit_at(logger, severity, text, strlen(text), file, line);
}

} // namespace

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // Locals, not namespace-scope objects: `cortex-m-rt` runs no C++ static
    // constructors (see the talker sibling).
    rclcpp::Node node;
    rclcpp::PollSubscription<std_msgs::msg::String> sub;

    if (!rclcpp::init_in(NROS_ENTRY_LOCATOR, static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID)).ok()) {
        emit(nros_log_default_logger(), NROS_LOG_SEVERITY_ERROR, "rclcpp::init failed", __FILE__,
             __LINE__);
        return 1;
    }
    if (!rclcpp::create_node(node, "listener").ok()) {
        emit(nros_log_default_logger(), NROS_LOG_SEVERITY_ERROR, "create_node failed", __FILE__,
             __LINE__);
        return 1;
    }
    nros_logger_t logger = node.get_logger();
    if (!node.create_subscription(sub, "/chatter").ok()) {
        emit(logger, NROS_LOG_SEVERITY_ERROR, "create_subscription failed", __FILE__, __LINE__);
        return 1;
    }
    emit(logger, NROS_LOG_SEVERITY_INFO, "Subscriber created for topic: /chatter", __FILE__,
         __LINE__);

    while (rclcpp::ok()) {
        rclcpp::spin_once(100);

        std_msgs::msg::String msg;
        while (sub.take(msg)) {
            // The marker the e2e greps for (`nros_tests::output::LISTENER_LOG_PREFIX`).
            char line[320];
            size_t len = append_str(line, sizeof(line), 0u, "I heard: [");
            len = append_str(line, sizeof(line), len, msg.data.c_str());
            len = append_str(line, sizeof(line), len, "]");
            line[len] = '\0';
            emit(logger, NROS_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
        }
    }
    rclcpp::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER()
