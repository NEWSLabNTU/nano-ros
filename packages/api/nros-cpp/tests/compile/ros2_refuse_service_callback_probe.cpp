/*
 * NEGATIVE probe — phase-417 W2.c, the callback-shape refusal, as phase-488
 * narrowed it.
 *
 * phase-488 TOOK upstream's `std::shared_ptr<Request>` / `std::shared_ptr<Response>`
 * handler (hosted, function pointer or capture-less lambda): the request and
 * response are lent from the dispatch frame through a no-op deleter and a
 * frame-local control block, so it needs no heap and no second delivery path —
 * the two reasons this probe used to exist for. `ported/minimal_service.cpp`
 * under `rclcpp-examples/` is the stock upstream file compiling, linking and
 * booting with that shape.
 *
 * What is STILL refused, and what this TU now holds: a handler with STATE —
 * a capturing lambda or a `std::bind` result. The arena stores one function
 * pointer per service, so there is nowhere for the captures to live. Absence
 * would give "no matching function for call to create_service"; the refusal
 * names both accepted shapes and `rclcpp::bind_service` instead.
 *
 * `just check cpp` requires this TU to FAIL. The shapes nano-ros takes are in
 * the POSITIVE probe (`ros2_api_adoption_stage2.cpp`) and in the ported
 * minimal_service example.
 */

#include <rclcpp/rclcpp.hpp>

#include <memory>

namespace {

struct AddTwoInts {
    struct Request {
        int64_t a = 0;
        int64_t b = 0;
        static const size_t SERIALIZED_SIZE_MAX = 16;
        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Request_";
        static constexpr const char* TYPE_HASH = "RIHS01_add_two_ints_request_stub";
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
            if (out) *out = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };
    struct Response {
        int64_t sum = 0;
        static const size_t SERIALIZED_SIZE_MAX = 8;
        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Response_";
        static constexpr const char* TYPE_HASH = "RIHS01_add_two_ints_response_stub";
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
            if (out) *out = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };
    static constexpr const char* TYPE_NAME = "example_interfaces::srv::dds_::AddTwoInts_";
    static constexpr const char* TYPE_HASH = "RIHS01_add_two_ints_stub";
};

// Upstream's handler signature, verbatim.
} // namespace

int ros2_refuse_service_callback_probe(rclcpp::Node& node);
int ros2_refuse_service_callback_probe(rclcpp::Node& node) {
    int64_t offset = 1;
    auto service = node.create_service<AddTwoInts>(
        "add_two_ints", [offset](const std::shared_ptr<AddTwoInts::Request> request,
                                 std::shared_ptr<AddTwoInts::Response> response) {
            response->sum = request->a + request->b + offset;
        });
    (void)service;
    return 0;
}
