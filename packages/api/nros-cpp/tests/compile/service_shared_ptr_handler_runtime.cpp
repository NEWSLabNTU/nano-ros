// phase-488 — upstream's shared_ptr service handler, lent from the dispatch
// frame. RUNTIME, because both halves of the contract are behaviour:
//
//   (1) the handler sees the deserialized request and its writes to the
//       response reach the serialized reply — the same bytes the reference
//       shape produces, through the same trampoline signature the arena calls;
//   (2) a handler that KEEPS a request/response `shared_ptr` past the call
//       aborts, naming why, instead of leaving a pointer into a dead frame.
//
// Header-only (the trampoline and the stderr log sink), so it links no
// nano-ros archive; `--unresolved-symbols=ignore-all` covers only the
// issue-0360 variant anchors, as for `failed_create_aborts.cpp`.

#include <sys/wait.h>
#include <unistd.h>

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>

#include <rclcpp/rclcpp.hpp>

#include "nros_log_stderr_sink.hpp"

namespace {

// A service whose FFI hooks copy the struct, so the trampoline's
// deserialize/serialize round trip is observable.
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

void expect(bool cond, const char* what) {
    if (!cond) {
        ::std::fprintf(stderr, "service_shared_ptr_handler_runtime: FAILED: %s\n", what);
        ::std::abort();
    }
}

int g_seen_header = -1;

void two_arg(std::shared_ptr<AddTwoInts::Request> request,
             std::shared_ptr<AddTwoInts::Response> response) {
    response->sum = request->a + request->b;
}

void three_arg(const std::shared_ptr<rmw_request_id_t> header,
               const std::shared_ptr<AddTwoInts::Request> request,
               const std::shared_ptr<AddTwoInts::Response> response) {
    g_seen_header = header ? static_cast<int>(header->sequence_number) : -2;
    response->sum = request->a * request->b;
}

std::shared_ptr<AddTwoInts::Request> g_kept;
void keeps_request(std::shared_ptr<AddTwoInts::Request> request,
                   std::shared_ptr<AddTwoInts::Response>) {
    g_kept = request; // upstream allows this; the lent object dies with the frame
}

template <int Shape, typename Fn> int64_t dispatch(Fn fn, int64_t a, int64_t b, bool* ok) {
    using S = AddTwoInts;
    static_assert(::rclcpp::detail::service_ptr_shape<S, Fn>::value == Shape,
                  "the handler must classify as the shape under test");
    S::Request req;
    req.a = a;
    req.b = b;
    uint8_t in[sizeof(S::Request)];
    size_t in_len = 0;
    expect(S::Request::ffi_serialize(&req, in, sizeof(in), &in_len) == 0, "serialize request");
    uint8_t out[64];
    size_t out_len = 0;
    void* ctx = ::rclcpp::detail::shared_ptr_service_context<S>(
        fn, ::rclcpp::tr::integral_constant<int, Shape>());
    *ok = ::rclcpp::detail::shared_ptr_service_trampoline<S, Shape>::call(
        in, in_len, out, sizeof(out), &out_len, ctx);
    S::Response resp;
    if (*ok) expect(S::Response::ffi_deserialize(out, out_len, &resp) == 0, "deserialize reply");
    return resp.sum;
}

} // namespace

int main() {
    // The reference shape still classifies as NEITHER pointer shape.
    using RefFn = void (*)(const AddTwoInts::Request&, AddTwoInts::Response&);
    static_assert(::rclcpp::detail::service_ptr_shape<AddTwoInts, RefFn>::value == 0,
                  "the reference shape keeps its own overload");

    bool ok = false;
    expect(dispatch<2>(&two_arg, 40, 2, &ok) == 42 && ok, "two-argument handler: 40 + 2");
    expect(dispatch<3>(&three_arg, 6, 7, &ok) == 42 && ok, "three-argument handler: 6 * 7");
    expect(g_seen_header == 0, "the request header is delivered, zero-filled");
    auto lambda = [](std::shared_ptr<AddTwoInts::Request> rq,
                     std::shared_ptr<AddTwoInts::Response> rs) { rs->sum = rq->a - rq->b; };
    expect(dispatch<2>(+lambda, 50, 8, &ok) == 42 && ok, "capture-less lambda handler");

    // (2) Retention aborts, loudly.
    int fds[2];
    expect(::pipe(fds) == 0, "pipe");
    ::fflush(nullptr);
    const pid_t pid = ::fork();
    if (pid == 0) {
        ::close(fds[0]);
        ::dup2(fds[1], 2);
        ::close(fds[1]);
        bool child_ok = false;
        (void)dispatch<2>(&keeps_request, 1, 2, &child_ok);
        ::_exit(17);
    }
    ::close(fds[1]);
    char buf[4096];
    size_t used = 0;
    for (;;) {
        const ssize_t n = ::read(fds[0], buf + used, sizeof(buf) - 1 - used);
        if (n <= 0) break;
        used += static_cast<size_t>(n);
        if (used >= sizeof(buf) - 1) break;
    }
    buf[used] = '\0';
    ::close(fds[0]);
    int status = 0;
    expect(::waitpid(pid, &status, 0) == pid, "waitpid");
    expect(WIFSIGNALED(status) && WTERMSIG(status) == SIGABRT,
           "a handler that keeps its request shared_ptr must ABORT the process");
    expect(::std::strstr(buf, "past the call") != nullptr,
           "the abort must say the handler kept the pointer past the call");
    expect(::std::strstr(buf, "NROS_LOG_STDERR_SINK_OVERSIZE") == nullptr,
           "the refusal must fit nros_log's buffer");

    ::std::printf("service_shared_ptr_handler_runtime: ok\n");
    return 0;
}
