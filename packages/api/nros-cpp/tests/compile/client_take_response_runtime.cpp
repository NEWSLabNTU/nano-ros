// issue 2148 -- `rclcpp::PollClient<S>::take_response(response, request_header)`,
// upstream `rclcpp::Client::take_response`: the next reply WITH its
// `rmw_request_id_t`, filled from the backend's `take_response_with_info`
// report (issue 2136). Before this a C++ client had no way to read a reply's
// header at all.
//
// A RUN on the stub backend, whose replies the test stages by id and whose
// reported request-writer GUID it sets (`nros_stub_rmw_set_reply_writer_guid`):
//
//   * nothing staged -> TryAgain, the header untouched;
//   * a reply -> its sum, the request's sequence number, the reported GUID;
//   * a reply whose backend reported no GUID -> the sequence number and an
//     all-zero GUID (never the previous reply's);
//   * replies taken in the order they arrive, each with ITS id;
//   * an unregistered client -> NotInitialized.
//
// Negative control, measured: the FFI writing `RawRequestId::default()`
// instead of `for_response(&info, seq)` fails steps 2-4.

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>

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
          "stage a reply on the stub");
}

bool guid_is(const rmw_request_id_t& h, const int8_t* want) {
    return std::memcmp(h.writer_guid, want, 16) == 0;
}

} // namespace

int main() {
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2148").ok()) {
        std::fprintf(stderr, "client_take_response_runtime: init on the stub backend failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);

    auto node = rclcpp::Node::make_shared("i2148_client");
    check(node && node->initialized(), "a node comes up on the stub backend");
    auto client = node->create_client<AddTwoInts>("add_two_ints");
    AddTwoInts::Request req;
    int8_t guid[16];
    for (int i = 0; i < 16; ++i)
        guid[i] = static_cast<int8_t>(0x30 + i);
    const int8_t zero[16] = {};

    // --- 1. nothing staged: TryAgain, the header untouched ------------------
    {
        nros_stub_rmw_reset_client();
        AddTwoInts::Response resp;
        rmw_request_id_t header;
        std::memset(&header, 0x5a, sizeof(header));
        rclcpp::Result r = client->take_response(resp, header);
        check(r.code() == rclcpp::ErrorCode::TryAgain, "no reply yet -> TryAgain");
        check(header.sequence_number == 0x5a5a5a5a5a5a5a5aLL, "TryAgain leaves the header alone");
    }

    // --- 2. a reply: its sum, its request's id, the reported GUID -----------
    {
        nros_stub_rmw_reset_client();
        nros_stub_rmw_set_reply_writer_guid(guid);
        (void)client->send_request(req);
        const int64_t seq = nros_stub_rmw_last_request_seq();
        stage_sum_for(seq, 42);
        AddTwoInts::Response resp;
        rmw_request_id_t header{};
        check(client->take_response(resp, header).ok(), "a staged reply is taken");
        check(resp.sum == 42, "the reply deserializes");
        check(header.sequence_number == seq, "the header names the request the reply answers");
        check(guid_is(header, guid), "the header carries the GUID the backend reported");
        check(nros_stub_rmw_staged_replies() == 0, "the reply was consumed");
    }

    // --- 3. no GUID reported: the sequence number, an all-zero GUID ---------
    {
        nros_stub_rmw_reset_client(); // clears the reported GUID
        (void)client->send_request(req);
        const int64_t seq = nros_stub_rmw_last_request_seq();
        stage_sum_for(seq, 7);
        AddTwoInts::Response resp;
        rmw_request_id_t header;
        std::memset(&header, 0x5a, sizeof(header));
        check(client->take_response(resp, header).ok(), "the second reply is taken");
        check(resp.sum == 7, "the second reply deserializes");
        check(header.sequence_number == seq, "the sequence number arrives without a GUID");
        check(guid_is(header, zero), "an unreported GUID is zero, never the last reply's");
    }

    // --- 4. two replies, OUT OF ORDER: each taken with ITS id ---------------
    {
        nros_stub_rmw_reset_client();
        nros_stub_rmw_set_reply_writer_guid(guid);
        (void)client->send_request(req);
        const int64_t seq_a = nros_stub_rmw_last_request_seq();
        (void)client->send_request(req);
        const int64_t seq_b = nros_stub_rmw_last_request_seq();
        stage_sum_for(seq_b, 200);
        stage_sum_for(seq_a, 100);
        AddTwoInts::Response resp;
        rmw_request_id_t header{};
        check(client->take_response(resp, header).ok(), "the first-arrived reply is taken");
        check(resp.sum == 200 && header.sequence_number == seq_b, "b's reply names b");
        check(client->take_response(resp, header).ok(), "the second-arrived reply is taken");
        check(resp.sum == 100 && header.sequence_number == seq_a, "a's reply names a");
        check(client->take_response(resp, header).code() == rclcpp::ErrorCode::TryAgain,
              "then nothing is left");
    }

    // --- 5. an unregistered client: NotInitialized --------------------------
    {
        rclcpp::PollClient<AddTwoInts> unregistered;
        AddTwoInts::Response resp;
        rmw_request_id_t header{};
        check(unregistered.take_response(resp, header).code() == rclcpp::ErrorCode::NotInitialized,
              "an unregistered client refuses");
    }

    rclcpp::shutdown();
    if (g_failures != 0) {
        std::fprintf(stderr, "client_take_response_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("client_take_response_runtime: OK\n");
    return 0;
}
