// Issue 2641 — `rclcpp::trace_marker` hands an application event to the
// installed trace sink, and refuses an id outside the application block.
//
// Compiled, linked against a `libnros_cpp.a` built WITH `trace-callbacks` and
// RUN by `just check cpp`. The sink is installed through the C seam
// (`rclc_set_trace_sink`), which the C++ archive carries: one sink serves both
// languages and the executor, which is the property under test.

#include "rclcpp/rclcpp.hpp"

#include <cstdint>
#include <cstdio>

extern "C" void rclc_set_trace_sink(void (*sink)(uint32_t, uint32_t));

// The registration hook every image owes; this harness opens no session.
extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {}

static int failures = 0;

#define CHECK(cond)                                                                                \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            std::fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);                   \
            failures++;                                                                            \
        }                                                                                          \
    } while (0)

static uint32_t ev_id[16];
static uint32_t ev_arg[16];
static int ev_n = 0;

extern "C" void capture(uint32_t id, uint32_t arg) {
    if (ev_n < 16) {
        ev_id[ev_n] = id;
        ev_arg[ev_n] = arg;
    }
    ev_n++;
}

int main() {
    static_assert(rclcpp::kTraceAppMarkerFirst == 1, "application block starts at 1");
    static_assert(rclcpp::kTraceAppMarkerLast == 15, "application block ends at 15");

    rclc_set_trace_sink(nullptr);
    CHECK(rclcpp::trace_marker(3, 7).ok());

    rclc_set_trace_sink(capture);
    CHECK(rclcpp::trace_marker(rclcpp::kTraceAppMarkerFirst, 0xdeadbeefu).ok());
    CHECK(rclcpp::trace_marker(rclcpp::kTraceAppMarkerLast, 42u).ok());
    for (uint32_t id = 16; id <= 27; id++) {
        rclcpp::Result r = rclcpp::trace_marker(id, 1u);
        CHECK(!r.ok());
        CHECK(r.code() == rclcpp::ErrorCode::InvalidArgument);
    }
    CHECK(!rclcpp::trace_marker(0, 1u).ok());
    rclc_set_trace_sink(nullptr);

    CHECK(ev_n == 2);
    CHECK(ev_id[0] == 1 && ev_arg[0] == 0xdeadbeefu);
    CHECK(ev_id[1] == 15 && ev_arg[1] == 42u);

    if (failures != 0) {
        std::fprintf(stderr, "trace_marker_runtime: %d failure(s)\n", failures);
        return 1;
    }
    std::printf("trace_marker_runtime: ok\n");
    return 0;
}
