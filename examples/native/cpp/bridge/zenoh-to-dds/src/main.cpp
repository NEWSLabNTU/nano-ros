/// @file main.cpp
/// @brief Phase 104.D.2 — C++ bridge example: forward raw CDR
///        bytes Zenoh → DDS.
///
/// Mirrors `examples/bridges/native-rust-zenoh-to-dds/src/main.rs`
/// (104.C.10) in C++. Demonstrates the rclcpp-aligned multi-Node
/// + multi-RMW pattern (104.C.3 + 104.C.9) at the nros-cpp
/// surface:
///
///   * Both RMW backends (zenoh-pico + dust-DDS) link into this
///     binary. Each self-registers under its canonical name via
///     its POSIX `.init_array` ctor (Phase 104.A.1+2).
///   * One `nros::Executor` holds two `nros::Node`s built via
///     the 104.C.9 NodeBuilder:
///       - "ingress" — binds to Zenoh via `.rmw("zenoh")`.
///       - "egress"  — binds to DDS  via `.rmw("dds")`.
///   * The executor opens a second session for the egress node
///     via 104.C.3 dispatch + drives both via `spin_once`.
///
/// Topic flow (untyped raw bytes — no codegen dep; bridge uses
/// `try_recv_raw` + `publish_raw`):
///
///   Zenoh "/chatter" ── ingress sub ──┐
///                                      ├─ bridge ─ publish_raw ──> DDS "/chatter"
///
/// Why Zenoh→DDS instead of Zenoh→Cyclone DDS (as the 104.D.2
/// spec originally suggested)? Cyclone DDS needs a one-time
/// `just cyclonedds setup` to build the upstream C++ library
/// + headers. Mirroring the existing 104.C.10 Rust template
/// (also Zenoh→DDS) keeps this example buildable on a fresh
/// `just setup` box with no additional steps. A Cyclone DDS
/// variant is a drop-in change: swap `.rmw("dds")` for
/// `.rmw("cyclonedds")` + add the `add_subdirectory(packages/dds/nros-rmw-cyclonedds)`
/// + whole-archive wrap (see README).

#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <cstring>

// Route NROS_TRY_RET through std::fprintf (we have stdio).
#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>

// ----------------------------------------------------------------------------
// Raw type info — std_msgs/String, matches the 104.C.10 Rust
// template's TYPE_NAME + TYPE_HASH constants. Hand-rolled so the
// bridge doesn't drag in a generated message crate; both the
// Publisher<M> + Subscription<M> templates only need the static
// constants below.
// ----------------------------------------------------------------------------

struct ChatterString {
    static constexpr const char* TYPE_NAME = "std_msgs/msg/dds_/String_";
    static constexpr const char* TYPE_HASH =
        "RIHS01_df668c740482bbd48fb39d76a70dfd4bd59db1288021743503259e948f6b1a18";
    // Upper bound for the per-recv stack buffer try_recv allocates.
    // The bridge uses try_recv_raw + publish_raw — this constant
    // isn't consulted on the raw path, but the typed template
    // still expects the symbol.
    static constexpr size_t SERIALIZED_SIZE_MAX = 4096;
};

// ----------------------------------------------------------------------------
// Signal handler — graceful Ctrl+C.
// ----------------------------------------------------------------------------

static volatile sig_atomic_t g_running = 1;
static void signal_handler(int /*signum*/) { g_running = 0; }

// ----------------------------------------------------------------------------
// Main.
// ----------------------------------------------------------------------------

int nros_app_main(int /*argc*/, char** /*argv*/) {
    std::printf("=== Phase 104.D.2 C++ bridge: Zenoh -> DDS ===\n");

    // Both backends self-register at lib load; primary picks
    // first-registered when `Executor::create` doesn't name one.
    // Pin primary to Zenoh by passing the Zenoh locator the
    // ingress node will inherit; the egress node overrides via
    // `.rmw("dds")` + opens its own session under the hood.
    const char* zenoh_locator = std::getenv("NROS_ZENOH_LOCATOR");
    if (!zenoh_locator) {
        zenoh_locator = "tcp/127.0.0.1:7447";
    }
    uint8_t domain_id = 0;
    if (const char* d = std::getenv("ROS_DOMAIN_ID")) {
        domain_id = static_cast<uint8_t>(std::atoi(d));
    }

    std::printf("Zenoh locator (ingress): %s\n", zenoh_locator);
    std::printf("DDS  locator (egress):   (dust-dds default)\n");
    std::printf("Domain ID: %d\n", domain_id);

    nros::Executor executor;
    NROS_TRY_RET(nros::Executor::create(executor, zenoh_locator, domain_id), 1);

    // Ingress node — Zenoh. NodeBuilder pattern from Phase
    // 104.C.9; `.rmw("zenoh")` binds the underlying session to
    // the named backend.
    nros::Node node_in;
    NROS_TRY_RET(executor.node_builder("ingress").rmw("zenoh").build(node_in), 1);
    std::printf("Ingress node bound to Zenoh\n");

    // Egress node — DDS. The executor opens an extra session
    // automatically when the rmw_name diverges from the primary
    // (Phase 104.C.3).
    nros::Node node_out;
    NROS_TRY_RET(executor.node_builder("egress").rmw("dds").build(node_out), 1);
    std::printf("Egress node bound to DDS\n");

    nros::Publisher<ChatterString> pub_out;
    NROS_TRY_RET(node_out.create_publisher(pub_out, "/chatter"), 1);
    std::printf("Egress raw publisher created on DDS /chatter\n");

    nros::Subscription<ChatterString> sub_in;
    NROS_TRY_RET(node_in.create_subscription(sub_in, "/chatter"), 1);
    std::printf("Ingress raw subscription registered on Zenoh /chatter\n");

    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    std::printf("\nBridge spinning (Ctrl+C to exit)...\n");
    std::printf("  publish on Zenoh /chatter; listen on DDS /chatter.\n\n");

    // Poll loop. nros-cpp's subscription API is poll-based —
    // `try_recv_raw` drains whatever the dispatch path queued
    // since the last call. `publish_raw` re-publishes the same
    // CDR bytes verbatim. No deserialise / reserialise.
    int forwarded = 0;
    uint8_t buf[ChatterString::SERIALIZED_SIZE_MAX];
    while (g_running && nros::ok()) {
        (void)executor.spin_once(100);
        size_t len = 0;
        nros::Result recv = sub_in.try_recv_raw(buf, sizeof(buf), len);
        if (recv.ok() && len > 0) {
            nros::Result snd = pub_out.publish_raw(buf, len);
            if (snd.ok()) {
                ++forwarded;
                std::printf("forwarded %zu bytes (count=%d)\n", len, forwarded);
            } else {
                std::fprintf(stderr, "publish_raw failed: %d (len=%zu)\n",
                             static_cast<int>(snd.code()), len);
            }
        }
    }

    std::printf("\nShutting down... forwarded %d messages total.\n", forwarded);
    return 0;
}

NROS_APP_MAIN_REGISTER_POSIX()
