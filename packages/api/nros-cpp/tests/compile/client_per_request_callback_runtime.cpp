// issue 2055 — the client most rclcpp components write, compiled unchanged:
//
//     rclcpp::Client<S>::SharedPtr client_;
//     client_ = create_client<S>("add_two_ints");
//     // in a timer callback:
//     if (!client_->wait_for_service(1s)) { return; }
//     client_->async_send_request(request, [](rclcpp::Client<S>::SharedFuture f) {...});
//
// RUNTIME, because the questions are behaviour: does the callback run, from
// the executor, with the reply staged for ITS request; may it send again from
// inside the callback; does a removed request's callback stay silent; and may
// `wait_for_service` be called inside a callback without starting a second
// dispatch pass (it used to call the executor's `spin_once` directly).
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
#include <vector>

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
    check(nros_stub_rmw_stage_reply_for(seq, reinterpret_cast<const uint8_t*>(&resp), sizeof(resp)),
          "stage a reply for a request id on the stub");
}

using Client = rclcpp::Client<AddTwoInts>;

/// ros2/demos `composition`'s client component, as upstream writes it, minus
/// the logging: a member `Client<S>::SharedPtr`, the request sent from a timer
/// with a per-request callback.
class ClientNode {
  public:
    explicit ClientNode(rclcpp::Node::SharedPtr node) : node_(node) {
        client_ = node_->create_client<AddTwoInts>("add_two_ints");
        timer_ = node_->create_wall_timer(std::chrono::milliseconds(5), [this]() { on_timer(); });
    }

    void on_timer() {
        if (sent_ >= 1) return;
        // Inside a callback: must answer without re-entering the executor.
        (void)client_->wait_for_service(std::chrono::milliseconds(20));
        waited_in_callback_ = true;
        auto request = std::make_shared<AddTwoInts::Request>();
        request->a = 2;
        request->b = 3;
        auto response_received_callback = [this](Client::SharedFuture future) {
            got_.push_back(future.get()->sum);
        };
        auto f = client_->async_send_request(request, response_received_callback);
        first_id_ = f.request_id;
        ++sent_;
    }

    rclcpp::Node::SharedPtr node_;
    Client::SharedPtr client_;
    rclcpp::TimerBase::SharedPtr timer_;
    int sent_ = 0;
    bool waited_in_callback_ = false;
    int64_t first_id_ = -1;
    std::vector<int64_t> got_;
};

void spin_for(const rclcpp::Node::SharedPtr& node, int ms) {
    const auto end = std::chrono::steady_clock::now() + std::chrono::milliseconds(ms);
    while (std::chrono::steady_clock::now() < end) {
        rclcpp::spin_some(node);
    }
}

} // namespace

int main() {
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2055").ok()) {
        std::fprintf(stderr, "client_per_request_callback_runtime: init on the stub failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);
    auto node = rclcpp::Node::make_shared("i2055_client");
    check(node && node->initialized(), "a node comes up on the stub backend");

    // --- the composition client: member SharedPtr, request from a timer ----
    ClientNode c(node);
    check(static_cast<bool>(c.client_),
          "Client<S>::SharedPtr holds what create_client<S>(name) returns");
    spin_for(node, 100);
    check(c.sent_ == 1, "the timer sent the request");
    check(c.waited_in_callback_, "wait_for_service returned inside the timer callback");
    check(c.got_.empty(), "no callback before a reply");
    stage_sum_for(c.first_id_, 5);
    spin_for(node, 100);
    check(c.got_.size() == 1 && c.got_[0] == 5,
          "the per-request callback ran from the executor with the reply for its request");
    spin_for(node, 50);
    check(c.got_.size() == 1, "...exactly once");

    // --- replies out of order, each callback gets its own ------------------
    {
        std::vector<int64_t> seen;
        auto req = std::make_shared<AddTwoInts::Request>();
        auto a = c.client_->async_send_request(
            req, [&](Client::SharedFuture f) { seen.push_back(f.get()->sum); });
        auto b = c.client_->async_send_request(
            req, [&](Client::SharedFuture f) { seen.push_back(100 + f.get()->sum); });
        check(a.valid() && b.valid() && a.request_id != b.request_id, "two requests, two ids");
        stage_sum_for(b.request_id, 2);
        stage_sum_for(a.request_id, 1);
        spin_for(node, 100);
        check(seen.size() == 2, "both callbacks ran");
        bool has_a = false, has_b = false;
        for (int64_t v : seen) {
            has_a = has_a || v == 1;
            has_b = has_b || v == 102;
        }
        check(has_a && has_b, "each callback got the reply staged for its own request");
    }

    // --- a callback that sends again ------------------------------------------
    {
        std::vector<int64_t> seen;
        auto req = std::make_shared<AddTwoInts::Request>();
        int64_t second = -1;
        Client::SharedPtr cli = c.client_;
        auto first = cli->async_send_request(req, [&](Client::SharedFuture f) {
            seen.push_back(f.get()->sum);
            second = cli->async_send_request(
                            req, [&](Client::SharedFuture g) { seen.push_back(g.get()->sum); })
                         .request_id;
        });
        stage_sum_for(first.request_id, 7);
        spin_for(node, 100);
        check(seen.size() == 1 && second >= 0, "the first callback ran and sent again");
        stage_sum_for(second, 8);
        spin_for(node, 100);
        check(seen.size() == 2 && seen[1] == 8,
              "the request sent from inside a callback got its own");
    }

    // --- a removed request's callback never runs ---------------------------
    {
        bool ran = false;
        auto req = std::make_shared<AddTwoInts::Request>();
        auto f = c.client_->async_send_request(req, [&](Client::SharedFuture) { ran = true; });
        check(c.client_->remove_pending_request(f.request_id), "the request was pending");
        stage_sum_for(f.request_id, 9);
        spin_for(node, 100);
        check(!ran, "a removed request's callback does not run");
    }

    if (g_failures != 0) {
        std::fprintf(stderr, "client_per_request_callback_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("client_per_request_callback_runtime: OK\n");
    return 0;
}
