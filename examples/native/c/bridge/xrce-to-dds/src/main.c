/// @file main.c
/// @brief Phase 104.D.1 — C bridge example: forward raw CDR bytes
///        XRCE → DDS.
///
/// Mirrors `examples/bridges/native-rust-zenoh-to-dds/src/main.rs`
/// in C. Demonstrates the rclcpp-aligned multi-Node + multi-RMW
/// pattern (Phase 104.C.3) at the nros-c surface:
///
///   * Both RMW backends (XRCE-DDS + dust-DDS) are linked into
///     this binary. Each self-registers under its canonical name
///     via its POSIX `.init_array` ctor (Phase 104.A.1+2).
///   * One `nros_executor_t` holds two `nros_node_t`s:
///       - "ingress" — binds to XRCE via `nros_node_options_t`
///         with `rmw_name = "xrce"`.
///       - "egress"  — binds to DDS  via `rmw_name = "dds"`.
///   * The executor opens a second session under the hood when
///     the egress node's rmw_name diverges from the primary —
///     stored in `extra_sessions` and driven by `spin_once`.
///
/// Topic flow (untyped raw bytes — no codegen dep):
///
///   XRCE "/chatter" ── ingress sub (raw) ──┐
///                                           ├─ bridge ─ publish_raw ──> DDS "/chatter"
///
/// Usage (POSIX):
///
///   # Spin up a Micro-XRCE agent on udp/127.0.0.1:8888 + a
///   # standard DDS discovery domain in another shell. Then:
///   ./xrce-to-dds-bridge

#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/subscription.h>
#include <nros/types.h>

// ----------------------------------------------------------------------------
// Raw type info — std_msgs/String, matches the Rust template's
// TYPE_NAME + TYPE_HASH constants. Inline so the example doesn't
// pull a generated message crate; the bridge forwards verbatim
// CDR bytes either way.
// ----------------------------------------------------------------------------

static const nros_message_type_t kStringType = {
    .type_name = "std_msgs/msg/dds_/String_",
    .type_hash = "RIHS01_df668c740482bbd48fb39d76a70dfd4bd59db1288021743503259e948f6b1a18",
    .serialized_size_max = 0,
};

// ----------------------------------------------------------------------------
// Application state — all in .bss to mirror the C example idiom.
// ----------------------------------------------------------------------------

typedef struct {
    nros_publisher_t* egress_publisher;
    int forwarded_count;
} bridge_context_t;

static struct {
    nros_support_t support;
    nros_executor_t executor;
    nros_node_t node_in;
    nros_node_t node_out;
    nros_publisher_t pub_out;
    nros_subscription_t sub_in;
    bridge_context_t bridge_ctx;
} app;

static volatile sig_atomic_t g_running = 1;
static nros_executor_t* g_executor = NULL;

// ----------------------------------------------------------------------------
// Signal handler — graceful Ctrl+C.
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor) {
        nros_executor_stop(g_executor);
    }
}

// ----------------------------------------------------------------------------
// Ingress subscription callback — receive raw CDR bytes on XRCE,
// republish verbatim on the DDS egress publisher.
// ----------------------------------------------------------------------------

static void on_ingress(const uint8_t* data, size_t len, void* context) {
    bridge_context_t* ctx = (bridge_context_t*)context;
    nros_ret_t ret = nros_publish_raw(ctx->egress_publisher, data, len);
    if (ret == NROS_RET_OK) {
        ctx->forwarded_count++;
        printf("forwarded %zu bytes (count=%d)\n", len, ctx->forwarded_count);
    } else {
        fprintf(stderr, "publish_raw failed: %d (len=%zu)\n", ret, len);
    }
}

// ----------------------------------------------------------------------------
// Helper — populate nros_node_options_t with rmw_name + namespace.
// nros-c's options struct uses fixed-size buffers + explicit length
// fields (matches the rclcpp-aligned C surface from Phase 104.C.8).
// ----------------------------------------------------------------------------

static nros_node_options_t make_options_for_rmw(const char* rmw) {
    nros_node_options_t opts = nros_node_get_default_options();
    size_t rmw_len = strlen(rmw);
    if (rmw_len > sizeof(opts.rmw_name)) {
        rmw_len = sizeof(opts.rmw_name);
    }
    memcpy(opts.rmw_name, rmw, rmw_len);
    opts.rmw_name_len = rmw_len;
    // Default namespace is "/" already.
    return opts;
}

// ----------------------------------------------------------------------------
// Main.
// ----------------------------------------------------------------------------

// Phase 156 Option 3 — explicit backend registration. The bridge
// CMakeLists sets `NANO_ROS_RMW=none` + pulls the XRCE + DDS
// staticlibs without `linkme-register`, so the auto-registration
// path is OFF (avoids the `nros-rmw-cffi` multi-monomorphisation
// linkme-duplicate-slice panic). Bridge declares the backend
// register C entry points + calls them before
// `nros_support_init` runs.
extern int8_t nros_rmw_xrce_register(void);
extern int8_t nros_rmw_dds_register(void);

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("=== Phase 104.D.1 bridge: XRCE -> DDS ===\n");

    // Phase 156 — explicit register both backends BEFORE
    // `nros_support_init` so the registry has both `"xrce"` +
    // `"dds"` names available for `nros_executor_node_init`'s
    // per-Node `.rmw(name)` dispatch.
    if (nros_rmw_xrce_register() != 0) {
        fprintf(stderr, "Failed to register XRCE RMW backend\n");
        return 1;
    }
    if (nros_rmw_dds_register() != 0) {
        fprintf(stderr, "Failed to register DDS RMW backend\n");
        return 1;
    }
    printf("Registered XRCE + DDS RMW backends\n");

    // Primary support context. The locator + domain_id supplied
    // here drive the executor's default-backend session — bridge
    // code doesn't use the primary session directly (both nodes
    // override `rmw_name` + open their own), but
    // `nros_support_init` is the entry point all C apps share.
    // Pull primary config from env so the same binary works
    // against any XRCE agent locator the operator supplies.
    const char* xrce_locator = getenv("NROS_XRCE_LOCATOR");
    if (!xrce_locator) {
        xrce_locator = "udp/127.0.0.1:8888";
    }
    const char* dds_locator = getenv("NROS_DDS_LOCATOR");
    // dust-DDS picks its own discovery transport by default
    // (UDPv4 multicast on the standard DDS domain); empty locator
    // = "use backend default".
    if (!dds_locator) {
        dds_locator = "";
    }
    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = 0;
    if (domain_str) {
        domain_id = (uint8_t)atoi(domain_str);
    }

    printf("XRCE locator (ingress): %s\n", xrce_locator);
    printf("DDS  locator (egress):  %s\n", dds_locator[0] ? dds_locator : "(backend default)");
    printf("Domain ID: %d\n", domain_id);

    memset(&app, 0, sizeof(app));

    // Initialise primary context against XRCE (matches the
    // rclcpp `node_builder.rmw("xrce")` setup but routes through
    // `nros_support_init` for the executor-bootstrap path).
    NROS_CHECK_RET(nros_support_init(&app.support, xrce_locator, domain_id), 1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    g_executor = &app.executor;

    // Ingress node — XRCE.
    {
        nros_node_options_t opts = make_options_for_rmw("xrce");
        NROS_CHECK_RET(
            nros_executor_node_init(&app.executor, &app.node_in, "ingress", &opts), 1);
        printf("Ingress node bound to XRCE\n");
    }

    // Egress node — DDS. Locator override flows through the
    // options struct so the second backend can reach a different
    // discovery domain.
    {
        nros_node_options_t opts = make_options_for_rmw("dds");
        size_t loc_len = strlen(dds_locator);
        if (loc_len > sizeof(opts.locator)) {
            loc_len = sizeof(opts.locator);
        }
        if (loc_len > 0) {
            memcpy(opts.locator, dds_locator, loc_len);
            opts.locator_len = loc_len;
        }
        NROS_CHECK_RET(
            nros_executor_node_init(&app.executor, &app.node_out, "egress", &opts), 1);
        printf("Egress node bound to DDS\n");
    }

    // Egress publisher — DDS side.
    NROS_CHECK_RET(
        nros_publisher_init(&app.pub_out, &app.node_out, &kStringType, "/chatter"), 1);
    printf("Egress raw publisher created on DDS /chatter\n");

    // Ingress subscription — XRCE side. Per-node session
    // dispatch already wired by Phase 104.C.8 — the underlying
    // `_on(NodeId, ...)` register variant picks the XRCE
    // session because `node_in.node_id` is non-zero.
    app.bridge_ctx.egress_publisher = &app.pub_out;
    app.bridge_ctx.forwarded_count = 0;
    NROS_CHECK_RET(
        nros_subscription_init(&app.sub_in, &app.node_in, &kStringType, "/chatter",
                               on_ingress, &app.bridge_ctx),
        1);
    NROS_CHECK_RET(nros_executor_register_subscription(&app.executor, &app.sub_in,
                                                       NROS_EXECUTOR_ON_NEW_DATA),
                   1);
    printf("Ingress raw subscription registered on XRCE /chatter\n");

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nBridge spinning (Ctrl+C to exit)...\n");
    printf("  publish on XRCE /chatter; listen on DDS /chatter.\n\n");

    // 100ms spin period — matches the C talker / listener
    // examples. The dual-session executor drives BOTH the
    // primary (XRCE) and the extra (DDS) sessions each tick
    // via `spin_once`.
    nros_ret_t ret = nros_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    printf("\nShutting down... forwarded %d messages total.\n", app.bridge_ctx.forwarded_count);
    nros_subscription_fini(&app.sub_in);
    nros_publisher_fini(&app.pub_out);
    nros_node_fini(&app.node_out);
    nros_node_fini(&app.node_in);
    nros_executor_fini(&app.executor);
    nros_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER_POSIX()
