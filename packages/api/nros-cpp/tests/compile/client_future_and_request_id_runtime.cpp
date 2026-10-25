// issue 2037 — upstream's `rclcpp::Client<S>::FutureAndRequestId` on the poll
// client: what `async_send_request` returns carries the future as `.future`
// and the request's sequence id as `.request_id` (C++17 structured bindings
// included), `share()` turns it into a `SharedFutureAndRequestId` whose
// `future` IS-A `std::shared_future<std::shared_ptr<Response>>`, and
// `remove_pending_request` takes the id or either wrapper.
//
// RUNTIME, because each of these compiles as an inert stub: a `request_id` that
// is always 0, a `share()` whose copies never complete or each get a different
// reply. The questions are behaviour — is `request_id` the id the reply is
// matched on (a reply staged FOR that id reaches this future), does every copy
// of the shared future see the same response, does removal by id invalidate
// the future, and does a removed shared request settle instead of waiting.
//
// Real `libnros_cpp.a` + the stub backend, whose replies the test stages by
// request id (`nros_stub_rmw_stage_reply_for`).

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

void stage_sum_for(int64_t seq, int64_t sum) {
    AddTwoInts::Response resp;
    resp.sum = sum;
    check(nros_stub_rmw_stage_reply_for(seq, reinterpret_cast<const uint8_t*>(&resp),
                                        sizeof(resp)),
          "stage a reply for a request id on the stub");
}

using Client = rclcpp::PollClient<AddTwoInts>;

} // namespace

int main() {
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2037").ok()) {
        std::fprintf(stderr, "client_future_and_request_id_runtime: init on the stub failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);
    auto node = rclcpp::Node::make_shared("i2037_client");
    check(node && node->initialized(), "a node comes up on the stub backend");
    auto client = node->create_client<AddTwoInts>("add_two_ints");
    auto request = std::make_shared<AddTwoInts::Request>();
    request->a = 1;
    request->b = 2;

    // --- .future and .request_id --------------------------------------------
    {
        Client::FutureAndRequestId f = client->async_send_request(request);
        const int64_t sent = nros_stub_rmw_last_request_seq();
        check(f.valid() && f.future.valid(), "async_send_request hands back a valid future");
        check(f.request_id == sent, ".request_id is the sequence id the request went out with");
        stage_sum_for(f.request_id, 3);
        check(rclcpp::spin_until_future_complete(node, f, 1000) ==
                  rclcpp::FutureReturnCode::SUCCESS,
              "spin_until_future_complete takes a FutureAndRequestId");
        std::shared_ptr<AddTwoInts::Response> r = f.future.get();
        check(r && r->sum == 3, ".future.get() returns the reply staged for .request_id");
        check(!f.valid(), "...and consumed the future");
    }

    // --- structured bindings, as upstream's struct allows --------------------
    {
        auto [future, id] = client->async_send_request(request);
        check(id == nros_stub_rmw_last_request_seq(), "auto [future, id] binds the request id");
        stage_sum_for(id, 30);
        check(future.wait_for(std::chrono::seconds(1)) == std::future_status::ready,
              "the bound future's wait_for drives the executor until the reply");
        auto r = future.get();
        check(r && r->sum == 30, "the bound future gets its own reply");
    }

    // --- two in flight, replies staged out of order, matched by request_id ---
    {
        auto a = client->async_send_request(request);
        auto b = client->async_send_request(request);
        check(a.request_id != b.request_id, "two requests carry two ids");
        stage_sum_for(b.request_id, 22);
        stage_sum_for(a.request_id, 11);
        check(a.wait_for(std::chrono::seconds(1)) == std::future_status::ready &&
                  b.wait_for(std::chrono::seconds(1)) == std::future_status::ready,
              "both complete");
        auto ra = a.get();
        auto rb = b.get();
        check(ra && ra->sum == 11 && rb && rb->sum == 22, "each id gets the reply staged for it");
    }

    // --- remove_pending_request(request_id) ---------------------------------
    {
        auto f = client->async_send_request(request);
        check(!client->remove_pending_request(f.request_id + 1000),
              "an id with no pending request removes nothing");
        check(f.valid(), "...and leaves the future pending");
        check(client->remove_pending_request(f.request_id), "removal by request_id");
        check(!f.valid(), "the removed request's future is no longer valid");
        check(!client->remove_pending_request(f.request_id), "removing it twice answers false");
        auto g = client->async_send_request(request);
        check(client->remove_pending_request(g), "remove_pending_request(FutureAndRequestId&)");
        check(!g.valid(), "...invalidates it");
    }

    // --- share(): one reply, every copy --------------------------------------
    {
        auto f = client->async_send_request(request);
        const int64_t id = f.request_id;
        Client::SharedFutureAndRequestId s = f.share();
        check(!f.valid(), "share() leaves the FutureAndRequestId empty, as upstream's");
        check(s.valid() && s.request_id == id, "the shared form keeps the request id");
        Client::SharedFutureAndRequestId copy = s;
        std::shared_future<std::shared_ptr<AddTwoInts::Response>> bare = s.future;
        check(!rclcpp::detail::future_is_ready(copy.future), "the shared future waits for its reply");
        stage_sum_for(id, 77);
        const bool done = rclcpp::spin_until_future_complete(node, s.future, 1000) ==
                          rclcpp::FutureReturnCode::SUCCESS;
        check(done, "spin_until_future_complete completes the shared future");
        if (!done) return 1; // get() would wait for ever on a future that never settles
        const auto& r1 = s.get();
        const auto& r2 = copy.get();
        check(r1 && r1->sum == 77, "the shared future gets the reply");
        check(r1.get() == r2.get(), "every copy gets the SAME response object");
        check(bare.wait_for(std::chrono::seconds(0)) == std::future_status::ready &&
                  bare.get().get() == r1.get(),
              "a bare std::shared_future copy completes with it too");
    }

    // --- a removed shared request settles to a null response ----------------
    {
        auto f = client->async_send_request(request);
        auto s = f.share();
        check(client->remove_pending_request(s), "remove_pending_request(SharedFutureAndRequestId)");
        const bool settled = s.wait_for(std::chrono::milliseconds(200)) == std::future_status::ready;
        check(settled, "a removed shared request settles instead of waiting");
        check(settled && !s.get(), "...to a null response");
    }

    // --- an empty future shares as an empty one ------------------------------
    {
        auto f = client->async_send_request(std::shared_ptr<AddTwoInts::Request>());
        check(!f.valid() && f.request_id == rclcpp::detail::SEQUENCE_ID_UNKNOWN,
              "a null request yields an empty future with no id");
        auto s = f.share();
        check(!s.valid(), "...which shares as an invalid shared future");
    }

    rclcpp::shutdown();
    if (g_failures != 0) {
        std::fprintf(stderr, "client_future_and_request_id_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("client_future_and_request_id_runtime: OK\n");
    return 0;
}
