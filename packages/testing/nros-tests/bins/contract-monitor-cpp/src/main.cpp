// phase-462 W1 (RFC-0052) -- the C++ twin of `contract-monitor-pub`
// (bins/contract-monitor/src/bin/pub.rs).
//
// The Rust twin declares ONE `MonitorSpec` by hand -- topic `/cm_header`, fqn
// `/cm/pub/cm_header`, 10 Hz declared minimum -- installs it with
// `Executor::set_monitor_table` before creating its publisher, publishes a
// `std_msgs/Header` every `CM_PERIOD_MS`, and drains the executor's violation
// ring each loop. This TU does the same four things through the C ABI a
// generated C++ entry uses: `nros_cpp_install_monitors` for the table (the
// rows below are the twin's row, spelled as `nros_cpp_monitor_row_t`), the
// stock `Publisher<Header>` for the publish (whose facade bumps the row's
// cell), and `nros_cpp_executor_drain_violations` for the drain.
//
// Output the parity test keys on:
//   `cm_pub_cpp: installed <n> monitor rows`        -- n is 1, or 0 with
//                                                     CM_CONTRACT=0
//   `cm_pub_cpp: row topic=<t> fqn=<f> min_rate_hz_milli=<m> max_latency_ms=<l>`
//   `DIAG rule=<rule> fqn=<fqn> measured=<m> declared=<d>` -- one per drained
//                                                     violation
//
// The Rust twin republishes its violations as `DiagnosticArray` through the
// nros-diagnostics reporter; that crate has no C++ surface, so this twin
// prints the same rule id the reporter would carry. The rule vocabulary is
// the runtime's (`rate-hierarchy-runtime`), identical on both twins.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <nros/app_main.h>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace {

constexpr const char* kHeaderTopic = "/cm_header";
constexpr const char* kPubFqn = "/cm/pub/cm_header";
// Declared publisher rate guarantee (milli-Hz): 10 Hz, as the Rust twin's
// `MIN_RATE_HZ_MILLI`.
constexpr uint32_t kMinRateHzMilli = 10000u;

// The one row, in the C spelling the generated entry bakes.
const nros_cpp_monitor_row_t kRows[1] = {
    {kHeaderTopic, kPubFqn, kMinRateHzMilli, 0u},
};
alignas(8) unsigned char g_row_storage[1 * NROS_CPP_MONITOR_ROW_STORAGE];

uint64_t env_u64(const char* key, uint64_t fallback) {
    const char* v = getenv(key);
    if (v == nullptr || *v == '\0') return fallback;
    char* end = nullptr;
    unsigned long long parsed = strtoull(v, &end, 10);
    return (end == v) ? fallback : static_cast<uint64_t>(parsed);
}

uint64_t monotonic_ms() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return static_cast<uint64_t>(ts.tv_sec) * 1000ull +
           static_cast<uint64_t>(ts.tv_nsec) / 1000000ull;
}

uint64_t epoch_us() {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    return static_cast<uint64_t>(ts.tv_sec) * 1000000ull +
           static_cast<uint64_t>(ts.tv_nsec) / 1000ull;
}

void on_violation(void* ctx, const nros_cpp_violation_t* v) {
    auto* drained = static_cast<unsigned*>(ctx);
    ++*drained;
    printf("DIAG rule=%.*s fqn=%.*s measured=%u declared=%u\n", static_cast<int>(v->rule_len),
           v->rule, static_cast<int>(v->fqn_len), v->fqn, static_cast<unsigned>(v->measured),
           static_cast<unsigned>(v->declared));
    fflush(stdout);
}

} // namespace

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    setvbuf(stdout, nullptr, _IOLBF, 0);

    const uint64_t period_ms = env_u64("CM_PERIOD_MS", 500);
    const uint64_t stale_ms = env_u64("CM_STALE_MS", 0);
    const uint64_t run_ms = env_u64("CM_RUN_MS", 16000);
    const bool contracted = env_u64("CM_CONTRACT", 1) != 0;

    printf("contract-monitor cpp pub (rate contract on %s)\n", kHeaderTopic);
    NROS_TRY_RET(nros::init(), 1);
    void* exec = nros::global_handle();
    if (exec == nullptr) {
        fprintf(stderr, "cm_pub_cpp: no executor handle\n");
        return 1;
    }

    // Install the table BEFORE the publisher exists (the cell attaches at
    // create). `CM_CONTRACT=0` is the uncontracted twin: zero rows, and the
    // same `Publisher<Header>` below then carries a null cell.
    nros_cpp_monitor_tables_t tables = {
        .rows = contracted ? kRows : nullptr,
        .n_rows = contracted ? 1u : 0u,
        .row_storage = contracted ? g_row_storage : nullptr,
        .row_storage_len = contracted ? sizeof(g_row_storage) : 0u,
        .ages = nullptr,
        .n_ages = 0u,
        .age_storage = nullptr,
        .age_storage_len = 0u,
    };
    nros_cpp_ret_t mret = nros_cpp_install_monitors(exec, &tables);
    if (mret != NROS_CPP_RET_OK) {
        fprintf(stderr, "cm_pub_cpp: install_monitors failed: %d\n", static_cast<int>(mret));
        return 1;
    }
    printf("cm_pub_cpp: installed %u monitor rows\n", static_cast<unsigned>(tables.n_rows));
    for (size_t i = 0; i < tables.n_rows; ++i) {
        printf("cm_pub_cpp: row topic=%s fqn=%s min_rate_hz_milli=%u max_latency_ms=%u\n",
               kRows[i].topic, kRows[i].fqn, static_cast<unsigned>(kRows[i].min_rate_hz_milli),
               static_cast<unsigned>(kRows[i].max_latency_ms));
    }

    rclcpp::Node node;
    NROS_TRY_RET(nros::create_node(node, "pub", "/cm"), 1);
    rclcpp::Publisher<std_msgs::msg::Header> pub;
    NROS_TRY_RET(node.create_publisher(pub, kHeaderTopic), 1);

    printf("cm_pub_cpp: period=%llums stale=%llums run=%llums contract=%d\n",
           static_cast<unsigned long long>(period_ms), static_cast<unsigned long long>(stale_ms),
           static_cast<unsigned long long>(run_ms), contracted ? 1 : 0);

    const uint64_t started = monotonic_ms();
    uint64_t last_pub = started - period_ms;
    unsigned seq = 0;
    unsigned drained = 0;
    while (monotonic_ms() - started < run_ms) {
        nros::spin_once(20);
        const uint64_t now = monotonic_ms();
        if (now - last_pub >= period_ms) {
            last_pub = now;
            const uint64_t stamp_us = epoch_us() - stale_ms * 1000ull;
            std_msgs::msg::Header hdr;
            hdr.stamp.sec = static_cast<int32_t>(stamp_us / 1000000ull);
            hdr.stamp.nanosec = static_cast<uint32_t>((stamp_us % 1000000ull) * 1000ull);
            hdr.frame_id = "cm";
            rclcpp::Result r = pub.publish(hdr);
            if (!r.ok()) {
                fprintf(stderr, "cm_pub_cpp: publish failed: %d\n", r.raw());
            }
            ++seq;
            if (seq % 4 == 0) {
                printf("cm_pub_cpp: published %u headers\n", seq);
            }
        }
        nros_cpp_ret_t dret = nros_cpp_executor_drain_violations(exec, on_violation, &drained);
        if (dret != NROS_CPP_RET_OK) {
            fprintf(stderr, "cm_pub_cpp: drain failed: %d\n", static_cast<int>(dret));
            return 1;
        }
    }
    printf("cm_pub_cpp: done (%u headers, %u violations)\n", seq, drained);
    rclcpp::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER()
