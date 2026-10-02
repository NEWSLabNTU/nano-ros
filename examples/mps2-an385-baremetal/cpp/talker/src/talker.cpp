/// @file talker.cpp
/// @brief C++ talker for the PURE BARE-METAL MPS2-AN385 — publishes
///        `std_msgs/String` "Hello World: N" from a 1 Hz timer.
///
/// The application is ordinary nano-ros C++, the same program as
/// `examples/mps2-an385-freertos/cpp/talker/src/main.cpp`. What differs is a
/// property of having no RTOS, not of the C++ API (issue 1512):
///
///  1. **No libc, no C++ runtime.** No `<stdio.h>`, no `<signal.h>`, no
///     exceptions, no RTTI. `nros-cpp`'s headers are alloc-free and take their
///     hosted-STL surface only when BOTH `__STDC_HOSTED__` and `__has_include`
///     say it exists, so `-ffreestanding` (build.rs) keeps it out.
///  2. **No `printf` substitution.** `nros-baremetal-common`'s `vsnprintf`
///     copies its format verbatim, so a `NROS_INFO("n=%d", n)` would print
///     `n=%d`. Records are built by hand and emitted through `nros_log_emit_at`,
///     exactly as the `c/talker` sibling does.
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

// ----------------------------------------------------------------------------
// Backend registration — the image's, exactly as on the cmake road
// ----------------------------------------------------------------------------
//
// `nros_cpp_init` (reached through `nros::init`) calls
// `nros_app_register_backends()` unconditionally, and every C/C++ image supplies
// the strong definition; `nano_ros_link_rmw()` generates this TU on the cmake
// road, and a cargo-rooted image writes it. Bare metal walks no `.init_array`.
extern "C" int nros_rmw_zenoh_register(void);

extern "C" void nros_app_register_backends(void) {
    (void)nros_rmw_zenoh_register();
}

namespace {

struct TalkerContext {
    rclcpp::Publisher<std_msgs::msg::String>* publisher;
    nros_logger_t logger;
    uint32_t count;
};

// ----------------------------------------------------------------------------
// Hand-rolled formatting — see the file header for why printf is unavailable
// ----------------------------------------------------------------------------
//
// Both take the buffer's CAPACITY and truncate rather than run past it; each
// leaves room for the caller's NUL. Same helpers as the `c/talker` sibling.

size_t append_u32(char* out, size_t cap, size_t pos, uint32_t value) {
    char digits[10];
    size_t n = 0;
    do {
        digits[n++] = static_cast<char>('0' + (value % 10u));
        value /= 10u;
    } while (value != 0u && n < sizeof(digits));
    while (n > 0u && pos + 1u < cap) {
        out[pos++] = digits[--n];
    }
    return pos;
}

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

// ----------------------------------------------------------------------------
// Timer callback — publish one message
// ----------------------------------------------------------------------------

void timer_callback(void* context) {
    TalkerContext* ctx = static_cast<TalkerContext*>(context);

    // Pre-increment so the first payload is "Hello World: 1", matching the
    // official ROS 2 demo talker and every sibling example in this tree.
    ctx->count++;
    char payload[64];
    size_t pos = append_str(payload, sizeof(payload), 0u, "Hello World: ");
    pos = append_u32(payload, sizeof(payload), pos, ctx->count);
    payload[pos] = '\0';

    std_msgs::msg::String msg;
    msg.data = payload;

    rclcpp::Result ret = ctx->publisher->publish(msg);
    if (!ret.ok()) {
        emit(ctx->logger, NROS_LOG_SEVERITY_ERROR, "publish failed", __FILE__, __LINE__);
        return;
    }

    // The marker the QEMU e2e greps for, spelled as every talker sibling spells
    // it (`nros_tests::output` owns the constant).
    char line[sizeof(payload) + 16u];
    size_t len = append_str(line, sizeof(line), 0u, "Publishing: '");
    len = append_str(line, sizeof(line), len, msg.data.c_str());
    len = append_str(line, sizeof(line), len, "'");
    line[len] = '\0';
    emit(ctx->logger, NROS_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
}

} // namespace

// ----------------------------------------------------------------------------
// Entry
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // The handles are LOCALS, as in every C++ sibling, and deliberately not
    // namespace-scope objects: `cortex-m-rt` zeroes `.bss` and copies `.data`
    // but runs no C++ static constructors (`.init_array`), so a global with a
    // non-trivial constructor would be used unconstructed. This frame lives for
    // the life of the image. The large storage (executor arena, entity slots)
    // is `nros-cpp`'s own `.bss`, not this stack.
    rclcpp::Node node;
    rclcpp::Publisher<std_msgs::msg::String> pub;
    nros::Timer timer;
    TalkerContext ctx = {};

    // `NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID` are the ONE producer of the
    // compile-time connect configuration (`<nros/entry_config.h>`, issue 0946).
    // Here the locator is the empty bottom rung, which the linked backend fills
    // with its own default; baking this image's `[image.*] locator` from
    // `system.toml` is what the runtime lane needs and is tracked on issue 1512.
    if (!nros::init(NROS_ENTRY_LOCATOR, static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID)).ok()) {
        emit(nros_log_default_logger(), NROS_LOG_SEVERITY_ERROR, "nros::init failed", __FILE__,
             __LINE__);
        return 1;
    }
    if (!nros::create_node(node, "talker").ok()) {
        emit(nros_log_default_logger(), NROS_LOG_SEVERITY_ERROR, "create_node failed", __FILE__,
             __LINE__);
        return 1;
    }
    ctx.logger = node.get_logger();
    if (!node.create_publisher(pub, "/chatter").ok()) {
        emit(ctx.logger, NROS_LOG_SEVERITY_ERROR, "create_publisher failed", __FILE__, __LINE__);
        return 1;
    }
    ctx.publisher = &pub;
    ctx.count = 0;
    if (!node.create_wall_timer(timer, 1000, timer_callback, &ctx).ok()) {
        emit(ctx.logger, NROS_LOG_SEVERITY_ERROR, "create_wall_timer failed", __FILE__, __LINE__);
        return 1;
    }

    // Spin forever. There is no signal to cancel on and no process to return
    // to: `app_main` returning is the end of the image, which `src/main.rs`
    // turns into a semihosting exit.
    while (rclcpp::ok()) {
        nros::spin_once(100);
    }
    rclcpp::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER()
