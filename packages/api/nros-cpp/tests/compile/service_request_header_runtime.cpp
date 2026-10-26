// issue 2081 -- rclcpp's three-argument service handler,
// `(std::shared_ptr<rmw_request_id_t> request_header, request, response)`,
// receives WHO sent the request: the client's writer GUID and the sequence
// number that client assigned, as the RMW backend reported them through its
// `take_request_with_info` slot. Before this the header was always zero.
//
// A RUN on the stub backend, which reports for a request what the test staged
// with it (`nros_stub_rmw_stage_request_info`), through the real executor:
//
//   * a function-pointer handler (the frame-lent trampoline) sees the id;
//   * a capturing lambda (the arena-captured trampoline) sees it too;
//   * a request staged with no id reaches the handler with a zero header;
//   * the two-argument form is unaffected, and every reply still goes out
//     against the backend's reply token, not the client's sequence number.
//
// Negative control, measured: `serve_lent` ignoring its `id` (the header left
// zero, the pre-2081 behaviour) is caught in `just check cpp` -- by the
// header-only `service_shared_ptr_handler_runtime`, which runs first. The
// arena half (the id built from the server's `request_info()`) is held by the
// C lane's `service_request_id.c`, whose mutation was measured there.

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>
#include <memory>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

int write_i32(int32_t v, uint8_t* buf, size_t cap, size_t* out) {
    if (cap < 8) return -1;
    const uint8_t hdr[4] = {0x00, 0x01, 0x00, 0x00};
    std::memcpy(buf, hdr, 4);
    std::memcpy(buf + 4, &v, 4);
    *out = 8;
    return 0;
}
int read_i32(const uint8_t* data, size_t len, int32_t* v) {
    if (len != 8) return -1;
    std::memcpy(v, data + 4, 4);
    return 0;
}

struct Echo {
    struct Request {
        int32_t value = 0;
        static constexpr const char* TYPE_NAME = "demo_msgs::srv::dds_::Echo_Request_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_echo";
        static constexpr size_t SERIALIZED_SIZE_MAX = 8;
        static int ffi_serialize(const void* m, uint8_t* buf, size_t cap, size_t* out) {
            return write_i32(static_cast<const Request*>(m)->value, buf, cap, out);
        }
        static int ffi_deserialize(const uint8_t* d, size_t len, void* m) {
            return read_i32(d, len, &static_cast<Request*>(m)->value);
        }
    };
    struct Response {
        int32_t value = 0;
        static constexpr const char* TYPE_NAME = "demo_msgs::srv::dds_::Echo_Response_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_echo";
        static constexpr size_t SERIALIZED_SIZE_MAX = 8;
        static int ffi_serialize(const void* m, uint8_t* buf, size_t cap, size_t* out) {
            return write_i32(static_cast<const Response*>(m)->value, buf, cap, out);
        }
        static int ffi_deserialize(const uint8_t* d, size_t len, void* m) {
            return read_i32(d, len, &static_cast<Response*>(m)->value);
        }
    };
    static constexpr const char* TYPE_NAME = "demo_msgs::srv::dds_::Echo_";
};

rmw_request_id_t g_fn_header;
int g_fn_calls = 0;

void fn_handler(const std::shared_ptr<rmw_request_id_t> header,
                const std::shared_ptr<Echo::Request> request,
                std::shared_ptr<Echo::Response> response) {
    ++g_fn_calls;
    g_fn_header = *header;
    response->value = request->value + 1;
}

void pump(const rclcpp::Node::SharedPtr& node) {
    for (int i = 0; i < 4; ++i)
        rclcpp::spin_some(node);
}

/// Stage a request and return the reply's value (-1 when none came back
/// against `seq`).
int32_t call(const rclcpp::Node::SharedPtr& node, const char* suffix, int64_t seq, int32_t value,
             const nros_stub_service_info_t* info) {
    uint8_t req[8];
    size_t len = 0;
    (void)write_i32(value, req, sizeof(req), &len);
    const bool staged = info != nullptr
                            ? nros_stub_rmw_stage_request_info(suffix, seq, req, len, info)
                            : nros_stub_rmw_stage_request(suffix, seq, req, len);
    check(staged, "stage a request");
    pump(node);
    int64_t got_seq = -1;
    size_t got_len = 0;
    const uint8_t* got = nros_stub_rmw_last_response(suffix, &got_seq, &got_len);
    int32_t answer = -1;
    if (got == nullptr || got_seq != seq || read_i32(got, got_len, &answer) != 0) return -1;
    return answer;
}

nros_stub_service_info_t make_info(int8_t first, int64_t seq) {
    nros_stub_service_info_t info;
    std::memset(&info, 0, sizeof(info));
    for (int i = 0; i < 16; ++i)
        info.writer_guid[i] = static_cast<int8_t>(first + i);
    info.sequence_number = seq;
    info.source_timestamp = 1700000000000000001LL;
    return info;
}

bool header_is(const rmw_request_id_t& h, const nros_stub_service_info_t& info) {
    return std::memcmp(h.writer_guid, info.writer_guid, 16) == 0 &&
           h.sequence_number == info.sequence_number;
}

bool header_is_zero(const rmw_request_id_t& h) {
    for (int i = 0; i < 16; ++i)
        if (h.writer_guid[i] != 0) return false;
    return h.sequence_number == 0;
}

} // namespace

int main() {
    nros_stub_rmw_set_accept_entities(true);
    // Staged requests have no arrival callback: the executor must poll.
    nros_stub_rmw_set_entity_wake_supported(false);
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2081").ok()) {
        std::fprintf(stderr, "service_request_header_runtime: init on the stub failed\n");
        return 1;
    }
    auto node = rclcpp::Node::make_shared("server", "/");
    check(node && node->initialized(), "a node comes up on the stub backend");

    // A function-pointer handler: the frame-lent trampoline.
    auto fn_srv = node->create_service<Echo>("fn_echo", &fn_handler);
    const nros_stub_service_info_t a = make_info(1, 4242);
    check(call(node, "/fn_echo", 7, 10, &a) == 11, "the function-pointer handler answered");
    check(g_fn_calls == 1, "...once");
    check(header_is(g_fn_header, a),
          "its request header is the client's writer GUID and sequence number");

    // No id reported: a zero header, never the previous request's.
    check(call(node, "/fn_echo", 8, 20, nullptr) == 21, "a request with no id is answered");
    check(header_is_zero(g_fn_header), "...with a zero request header");

    // A capturing lambda: the arena-captured trampoline.
    rmw_request_id_t lambda_header;
    std::memset(&lambda_header, 0, sizeof(lambda_header));
    int lambda_calls = 0;
    auto lambda_srv = node->create_service<Echo>(
        "lambda_echo",
        [&lambda_header, &lambda_calls](const std::shared_ptr<rmw_request_id_t> header,
                                        const std::shared_ptr<Echo::Request> request,
                                        std::shared_ptr<Echo::Response> response) {
            ++lambda_calls;
            lambda_header = *header;
            response->value = request->value * 2;
        });
    const nros_stub_service_info_t b = make_info(-40, 99);
    check(call(node, "/lambda_echo", 3, 21, &b) == 42, "the capturing handler answered");
    check(lambda_calls == 1, "...once");
    check(header_is(lambda_header, b), "the capturing handler's header carries the request id");

    // The two-argument form is unaffected.
    int two_calls = 0;
    auto two_srv = node->create_service<Echo>(
        "two_echo", [&two_calls](const std::shared_ptr<Echo::Request> request,
                                 std::shared_ptr<Echo::Response> response) {
            ++two_calls;
            response->value = request->value - 1;
        });
    check(call(node, "/two_echo", 11, 43, &a) == 42 && two_calls == 1,
          "the two-argument handler still answers");

    (void)fn_srv;
    (void)lambda_srv;
    (void)two_srv;
    rclcpp::shutdown();
    if (g_failures != 0) {
        std::fprintf(stderr, "service_request_header_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("service_request_header_runtime: OK\n");
    return 0;
}
