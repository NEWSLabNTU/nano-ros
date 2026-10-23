// issue 1996 — upstream's service-client FUTURE shape on the poll client:
// `async_send_request(std::shared_ptr<Request>)`, `future.get()` returning a
// `std::shared_ptr<Response>`, `future.wait_for(duration)` returning
// `std::future_status`, `future.valid()`, and
// `client->remove_pending_request(future)`.
//
// RUNTIME, because every one of these compiles as an inert stub: a `get()`
// that always returns nullptr, a `wait_for` that returns at once, a
// `remove_pending_request` that answers true for anything. The questions are
// behaviour — does the reply reach the shared_ptr, does `wait_for` really wait
// its budget (a LOWER bound, not only an upper one) and drive the executor
// until the reply lands, and does removal answer for THIS client's pending
// request only.
//
// Real `libnros_cpp.a` + the stub backend, whose `send_request` /
// `take_response` pair serves a reply the test STAGES
// (`nros_stub_rmw_stage_reply`).

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <chrono>
#include <cstdio>
#include <cstring>
#include <future>
#include <memory>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

// A service whose FFI hooks copy the struct, so the staged reply bytes ARE
// the response the future deserializes.
struct AddTwoInts {
    struct Request {
        int64_t a = 0;
        int64_t b = 0;
        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Request_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub";
        static const size_t SERIALIZED_SIZE_MAX = sizeof(int64_t) * 2;
        static int ffi_serialize(const void* msg, uint8_t* buf, size_t cap, size_t* out) {
            if (cap < sizeof(Request)) return -1;
            ::std::memcpy(buf, msg, sizeof(Request));
            *out = sizeof(Request);
            return 0;
        }
        static int ffi_deserialize(const uint8_t* data, size_t len, void* out) {
            if (len != sizeof(Request)) return -1;
            ::std::memcpy(out, data, sizeof(Request));
            return 0;
        }
    };
    struct Response {
        int64_t sum = 0;
        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Response_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub";
        static const size_t SERIALIZED_SIZE_MAX = sizeof(int64_t);
        static int ffi_serialize(const void* msg, uint8_t* buf, size_t cap, size_t* out) {
            if (cap < sizeof(Response)) return -1;
            ::std::memcpy(buf, msg, sizeof(Response));
            *out = sizeof(Response);
            return 0;
        }
        static int ffi_deserialize(const uint8_t* data, size_t len, void* out) {
            if (len != sizeof(Response)) return -1;
            ::std::memcpy(out, data, sizeof(Response));
            return 0;
        }
    };
    static constexpr const char* TYPE_NAME = "example_interfaces::srv::dds_::AddTwoInts_";
};

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

void stage_sum(int64_t sum) {
    AddTwoInts::Response resp;
    resp.sum = sum;
    check(nros_stub_rmw_stage_reply(reinterpret_cast<const uint8_t*>(&resp), sizeof(resp)),
          "stage a reply on the stub");
}

using Clock = std::chrono::steady_clock;

} // namespace

int main() {
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i1996").ok()) {
        std::fprintf(stderr, "client_future_shape_runtime: init on the stub backend failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);

    auto node = rclcpp::Node::make_shared("i1996_client");
    check(node && node->initialized(), "a node comes up on the stub backend");
    auto client = node->create_client<AddTwoInts>("add_two_ints");
    auto other = node->create_client<AddTwoInts>("add_two_ints_other");

    // --- async_send_request(shared_ptr) -------------------------------------
    auto request = std::make_shared<AddTwoInts::Request>();
    request->a = 41;
    request->b = 1;
    const uint32_t sent_before = nros_stub_rmw_requests_sent();
    auto result_future = client->async_send_request(request);
    check(result_future.valid(), "async_send_request hands back a valid future");
    check(nros_stub_rmw_requests_sent() == sent_before + 1,
          "async_send_request reached the backend's send_request");

    // --- get() before the reply: never blocks, refuses, does NOT consume -----
    check(!result_future.get(), "get() before the reply returns nullptr");
    check(result_future.valid(), "a refused get() leaves the future pending");

    // --- wait_for: waits its budget (lower bound) when nothing arrives -------
    {
        const auto t0 = Clock::now();
        const std::future_status st = result_future.wait_for(std::chrono::milliseconds(30));
        const auto waited = Clock::now() - t0;
        check(st == std::future_status::timeout, "wait_for times out with no reply");
        check(waited >= std::chrono::milliseconds(30), "wait_for WAITED its whole budget");
        check(waited < std::chrono::seconds(2), "wait_for did not overrun its budget wildly");
    }

    // --- the reply lands: wait_for answers ready well inside its budget ------
    stage_sum(42);
    {
        const auto t0 = Clock::now();
        const std::future_status st = result_future.wait_for(std::chrono::seconds(5));
        check(st == std::future_status::ready, "wait_for answers ready once the reply is in");
        check(Clock::now() - t0 < std::chrono::seconds(5), "wait_for returned before its budget");
    }
    check(rclcpp::spin_until_future_complete(node, result_future) ==
              rclcpp::FutureReturnCode::SUCCESS,
          "spin_until_future_complete takes the future and reports SUCCESS");

    // --- get(): the reply as an OWNING shared_ptr, consuming the future ------
    auto result = result_future.get();
    check(static_cast<bool>(result), "get() after SUCCESS returns the reply");
    check(result && result->sum == 42, "get()'s shared_ptr carries the deserialized reply");
    check(!result_future.valid(), "get() consumed the future");
    check(!result_future.get(), "a second get() returns nullptr");
    check(result_future.wait_for(std::chrono::seconds(5)) == std::future_status::timeout,
          "wait_for on a consumed future answers timeout");
    std::shared_ptr<AddTwoInts::Response> kept = result;
    result.reset();
    check(kept && kept->sum == 42, "the reply outlives the future (it is owned)");

    // --- remove_pending_request ---------------------------------------------
    auto pending = client->async_send_request(request);
    check(pending.valid(), "a second request is pending");
    auto foreign = other->async_send_request(request);
    check(!client->remove_pending_request(foreign),
          "remove_pending_request refuses another client's future");
    check(foreign.valid(), "...and leaves it pending");
    check(client->remove_pending_request(pending), "remove_pending_request removes ours");
    check(!pending.valid(), "a removed request's future is no longer valid");
    check(!client->remove_pending_request(pending), "removing it twice answers false");
    check(other->remove_pending_request(foreign), "the other client removes its own");

    // --- a zero budget is one check, and a null request sends nothing --------
    stage_sum(7);
    auto quick = client->async_send_request(request);
    check(quick.wait_for(std::chrono::milliseconds(0)) == std::future_status::ready,
          "wait_for(0) sees a reply that is already in");
    auto quick_reply = quick.get();
    check(quick_reply && quick_reply->sum == 7, "and get() returns it");
    const uint32_t sent_mid = nros_stub_rmw_requests_sent();
    auto null_future = client->async_send_request(std::shared_ptr<AddTwoInts::Request>());
    check(!null_future.valid(), "a null request yields an empty future");
    check(nros_stub_rmw_requests_sent() == sent_mid, "a null request sends nothing");

    rclcpp::shutdown();
    if (g_failures != 0) {
        std::fprintf(stderr, "client_future_shape_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("client_future_shape_runtime: OK\n");
    return 0;
}
