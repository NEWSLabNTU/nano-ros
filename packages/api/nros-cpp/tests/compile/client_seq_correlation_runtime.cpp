// issue 2009 — a poll client's futures are keyed by their request's SEQUENCE
// ID. Before this, every future was "slot 0": two requests in flight on one
// client took whichever reply landed first, and a reply that arrived after
// `remove_pending_request` was taken by the client's next future.
//
// RUNTIME, because every property here is behaviour: which future a reply
// reaches when replies arrive out of order, what happens to a reply nobody is
// waiting for any more (removed, destroyed, pruned), and what the client does at
// the transport's limit or on a backend that cannot assign ids.
//
// Real `libnros_cpp.a` + the stub backend, whose replies the test STAGES BY ID
// (`nros_stub_rmw_stage_reply_for`), in any order.

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <chrono>
#include <cstdio>
#include <cstring>
#include <memory>
#include <utility>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

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

using Fut = rclcpp::Future<AddTwoInts::Response>;

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
          "stage a reply on the stub");
}

int64_t take_sum(Fut& f) {
    AddTwoInts::Response out;
    if (!f.try_take(out).ok()) return -999;
    return out.sum;
}

} // namespace

int main() {
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2009").ok()) {
        std::fprintf(stderr, "client_seq_correlation_runtime: init on the stub backend failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);

    auto node = rclcpp::Node::make_shared("i2009_client");
    check(node && node->initialized(), "a node comes up on the stub backend");
    auto client = node->create_client<AddTwoInts>("add_two_ints");
    AddTwoInts::Request req;

    // --- 1. two in flight, replies OUT OF ORDER, the FIRST future polls ------
    // `a` polls first and takes `b`'s reply off the wire: it must hand it to
    // `b`, not keep it.
    {
        nros_stub_rmw_reset_client();
        Fut a = client->send_request(req);
        const int64_t seq_a = nros_stub_rmw_last_request_seq();
        Fut b = client->send_request(req);
        const int64_t seq_b = nros_stub_rmw_last_request_seq();
        check(a.valid() && b.valid(), "two requests in flight on one client");
        check(seq_a != seq_b, "the two requests went out under different ids");
        stage_sum_for(seq_b, 200);
        stage_sum_for(seq_a, 100);
        check(a.is_ready(), "a's reply is found behind b's");
        check(b.is_ready(), "b was handed its reply when a took it off the wire");
        check(take_sum(a) == 100, "a gets a's reply");
        check(take_sum(b) == 200, "b gets b's reply");
        check(nros_stub_rmw_staged_replies() == 0, "both replies were consumed");
    }

    // --- 2. the same, but the SECOND future polls first ---------------------
    {
        nros_stub_rmw_reset_client();
        Fut a = client->send_request(req);
        const int64_t seq_a = nros_stub_rmw_last_request_seq();
        Fut b = client->send_request(req);
        const int64_t seq_b = nros_stub_rmw_last_request_seq();
        stage_sum_for(seq_a, 11);
        stage_sum_for(seq_b, 22);
        check(b.is_ready(), "b's reply is found behind a's");
        check(take_sum(b) == 22, "b gets b's reply even though a's arrived first");
        check(a.is_ready(), "a was handed its reply");
        check(take_sum(a) == 11, "a gets a's reply");
    }

    // --- 3. a reply after remove_pending_request is DROPPED -----------------
    {
        nros_stub_rmw_reset_client();
        Fut removed = client->send_request(req);
        const int64_t seq_removed = nros_stub_rmw_last_request_seq();
        check(client->remove_pending_request(removed), "remove_pending_request removes it");
        Fut next = client->send_request(req);
        const int64_t seq_next = nros_stub_rmw_last_request_seq();
        stage_sum_for(seq_removed, 666); // the removed request's late reply
        check(!next.is_ready(), "the removed request's reply does NOT answer the next future");
        check(nros_stub_rmw_staged_replies() == 0, "the late reply was taken and dropped");
        stage_sum_for(seq_next, 7);
        check(next.is_ready(), "the next future still gets its own reply");
        check(take_sum(next) == 7, "with its own value");
    }

    // --- 4. a DESTROYED future's reply is dropped too -----------------------
    {
        nros_stub_rmw_reset_client();
        int64_t seq_gone = 0;
        {
            Fut gone = client->send_request(req);
            seq_gone = nros_stub_rmw_last_request_seq();
        }
        Fut live = client->send_request(req);
        const int64_t seq_live = nros_stub_rmw_last_request_seq();
        stage_sum_for(seq_gone, 5);
        stage_sum_for(seq_live, 6);
        check(live.is_ready() && take_sum(live) == 6,
              "a destroyed future's reply is skipped, the live one's delivered");
    }

    // --- 5. a future MOVED while pending still receives its reply -----------
    {
        nros_stub_rmw_reset_client();
        Fut other = client->send_request(req);
        const int64_t seq_other = nros_stub_rmw_last_request_seq();
        Fut original = client->send_request(req);
        const int64_t seq_moved = nros_stub_rmw_last_request_seq();
        Fut moved(std::move(original));
        check(!original.valid(), "the moved-from future is empty");
        Fut assigned;
        assigned = std::move(moved);
        stage_sum_for(seq_moved, 31);
        stage_sum_for(seq_other, 32);
        check(other.is_ready(), "the other future finds its reply");
        check(assigned.is_ready(), "the twice-moved future was handed its reply at its NEW address");
        check(take_sum(assigned) == 31, "and the value is its own");
        check(take_sum(other) == 32, "the other future's value is its own");
    }

    // --- 6. AT THE TRANSPORT'S LIMIT: the send is refused, nothing linked ----
    // The pending list has no capacity of its own (its entries are the futures);
    // what caps it is the backend's outstanding-request limit, simulated here.
    {
        nros_stub_rmw_reset_client();
        nros_stub_rmw_set_max_outstanding(2);
        const uint32_t sent0 = nros_stub_rmw_requests_sent();
        Fut a = client->send_request(req);
        const int64_t seq_a = nros_stub_rmw_last_request_seq();
        Fut b = client->send_request(req);
        Fut refused = client->send_request(req);
        check(a.valid() && b.valid(), "two requests fit the transport's limit");
        check(!refused.valid(), "the third is refused: an empty future");
        check(nros_stub_rmw_requests_sent() == sent0 + 2, "the refused request was never sent");
        stage_sum_for(seq_a, 41);
        check(a.is_ready() && take_sum(a) == 41, "the first still completes");
        Fut after = client->send_request(req);
        check(after.valid(), "an answered request frees room for the next send");
        nros_stub_rmw_set_max_outstanding(0);
    }

    // --- 7. a backend that CANNOT assign ids: one in flight, defined drops ---
    {
        nros_stub_rmw_reset_client();
        nros_stub_rmw_set_ids_supported(false);
        const uint32_t sent0 = nros_stub_rmw_requests_sent();
        Fut a = client->send_request(req);
        Fut b = client->send_request(req);
        check(a.valid(), "an id-less backend still sends one request");
        check(!b.valid(), "a second request is refused while the first has no id");
        check(nros_stub_rmw_requests_sent() == sent0 + 1, "and the refused one was never sent");
        stage_sum_for(0, 51);
        check(a.is_ready() && take_sum(a) == 51, "the sole pending request gets the unkeyed reply");

        Fut removed = client->send_request(req);
        check(client->remove_pending_request(removed), "remove an unkeyed request");
        Fut next = client->send_request(req);
        check(next.valid(), "a new request after the removal is allowed");
        stage_sum_for(0, 999); // may be the removed request's late reply
        check(!next.is_ready(), "the next unkeyed reply is dropped, not mismatched");
        stage_sum_for(0, 52);
        check(next.is_ready() && take_sum(next) == 52, "the one after it is delivered");
        nros_stub_rmw_set_ids_supported(true);
    }

    // --- 8. a pending future outlives its client: invalid, not dangling -----
    {
        nros_stub_rmw_reset_client();
        Fut orphan;
        {
            rclcpp::PollClient<AddTwoInts> scoped;
            check(node->create_client(scoped, "scoped_add").ok(), "a scoped poll client");
            orphan = scoped.send_request(req);
            check(orphan.valid(), "a request in flight on the scoped client");
        }
        check(!orphan.valid(), "the client is gone: the future is no longer valid");
        check(!orphan.is_ready(), "and never becomes ready");
    }

    // --- 9. prune_pending_requests forgets them all; their replies drop -----
    {
        nros_stub_rmw_reset_client();
        Fut done = client->send_request(req);
        const int64_t seq_done = nros_stub_rmw_last_request_seq();
        Fut p1 = client->send_request(req);
        const int64_t seq_p1 = nros_stub_rmw_last_request_seq();
        Fut p2 = client->send_request(req);
        stage_sum_for(seq_done, 81);
        check(done.is_ready(), "one reply arrived before the prune");
        check(client->prune_pending_requests() == 2, "prune reports the two still pending");
        check(!p1.valid() && !p2.valid(), "the pruned futures are no longer valid");
        check(done.valid() && take_sum(done) == 81, "an answered future keeps its reply");
        check(client->prune_pending_requests() == 0, "nothing left to prune");
        Fut after = client->send_request(req);
        const int64_t seq_after = nros_stub_rmw_last_request_seq();
        stage_sum_for(seq_p1, 82); // a pruned request's late reply
        stage_sum_for(seq_after, 83);
        check(after.is_ready() && take_sum(after) == 83,
              "a pruned request's late reply is dropped, the next future gets its own");
    }

    // --- 10. the blocking call_polling drops a cancelled future's reply -----
    {
        nros_stub_rmw_reset_client();
        Fut cancelled = client->send_request(req);
        const int64_t seq_cancelled = nros_stub_rmw_last_request_seq();
        cancelled.cancel();
        stage_sum_for(seq_cancelled, 71);       // the cancelled request's late reply
        stage_sum_for(seq_cancelled + 1, 72);   // the reply to call_polling's own request
        AddTwoInts::Response resp;
        check(client->call_polling(req, resp, 200).ok(), "call_polling completes");
        check(resp.sum == 72, "call_polling returns ITS reply, not the cancelled request's");
    }

    rclcpp::shutdown();
    if (g_failures != 0) {
        std::fprintf(stderr, "client_seq_correlation_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("client_seq_correlation_runtime: OK\n");
    return 0;
}
