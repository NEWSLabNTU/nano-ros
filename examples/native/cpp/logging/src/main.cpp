/// @file main.cpp
/// @brief Phase 88.13 — minimal nros::Node logging demo.
///
/// Walks the Phase-88 macros (`NROS_LOG_TRACE` … `NROS_LOG_FATAL`)
/// against the Node's Logger handle. The first emit auto-installs
/// `PlatformSink` so records reach the per-platform delivery
/// (`nros_platform_log_write` → POSIX stderr by default).
///
/// Run after starting zenohd:
///
///     zenohd --listen tcp/127.0.0.1:7447
///     NROS_LOCATOR=tcp/127.0.0.1:7447 ./build/cpp_logging

#include <cstdio>
#include <cstdlib>

#include <nros/nros.hpp>
#include <nros/log.hpp>

int main(int argc, char** argv) {
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
    std::setvbuf(stdout, nullptr, _IOLBF, 0);
    (void)argc;
    (void)argv;

    // Phase 212.M.2 — `nros::init()` (no-arg) pulls locator + domain_id
    // from `$NROS_LOCATOR` / `$ROS_DOMAIN_ID` at runtime.
    auto init = nros::init();
    if (!init.ok()) {
        std::fprintf(stderr, "nros::init failed: %d\n", init.raw());
        return 1;
    }

    nros::Node node;
    auto created = nros::create_node(node, "demo");
    if (!created.ok()) {
        std::fprintf(stderr, "create_node failed: %d\n", created.raw());
        nros::shutdown();
        return 1;
    }

    auto logger = node.get_logger();
    if (logger == nullptr) {
        std::fprintf(stderr, "node.get_logger() returned NULL\n");
        nros::shutdown();
        return 1;
    }

    /* Default per-Logger threshold = Info — TRACE/DEBUG drop here. */
    NROS_LOG_TRACE(logger, "round 1: trace=%d (dropped at default Info)", 1);
    NROS_LOG_DEBUG(logger, "round 1: debug=%d (dropped at default Info)", 1);
    NROS_LOG_INFO(logger, "round 1: info=%d", 1);
    NROS_LOG_WARN(logger, "round 1: warn=%d", 1);
    NROS_LOG_ERROR(logger, "round 1: error=%d", 1);
    NROS_LOG_FATAL(logger, "round 1: fatal=%d", 1);

    nros::shutdown();
    return 0;
}
