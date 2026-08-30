// Compile regression for issue 0278 Half B: `nros::Client<Svc>::call_polling`
// — a bounded service call that does NOT spin the executor, so it is safe from
// inside a subscription/timer callback (on a multi-threaded backend).
//
// The header `-fsyntax-only` loop in `just check cpp` only PARSES the templates;
// this TU instantiates `Client<AddTwoInts>::call_polling` against a
// generated-shape service type, so the method BODY (serialize req →
// nros_cpp_service_client_call_raw(..., timeout_ms) → deserialize resp) is
// type-checked, including the new `timeout_ms` FFI parameter.
// `just check cpp` compiles this with `-fsyntax-only -std=c++14`.
#include <nros/nros.hpp>

#include <type_traits>

namespace nros_cpp_service_client_call_polling_compile_test {

// Mirror of a codegen'd service (cf. example_interfaces/srv/AddTwoInts).
struct AddTwoInts {
    struct Request {
        int64_t a{0};
        int64_t b{0};
        static const size_t SERIALIZED_SIZE_MAX = 16;
        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Request_";
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
            if (out) *out = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };
    struct Response {
        int64_t sum{0};
        static const size_t SERIALIZED_SIZE_MAX = 16;
        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Response_";
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
            if (out) *out = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };
    static constexpr const char* TYPE_NAME = "example_interfaces::srv::dds_::AddTwoInts_";
    static constexpr const char* TYPE_HASH = "RIHS01_add_two_ints_stub";
};

// Force instantiation of call_polling (body type-checked at compile).
inline ::nros::Result instantiate(::nros::Client<AddTwoInts>& client) {
    AddTwoInts::Request req;
    AddTwoInts::Response resp;
    // The callback-safe bounded call with an explicit timeout.
    return client.call_polling(req, resp, /*timeout_ms=*/10);
}

// API-shape assertion — call_polling returns nros::Result and takes a timeout.
static_assert(std::is_same<decltype(std::declval<::nros::Client<AddTwoInts>&>().call_polling(
                               std::declval<const AddTwoInts::Request&>(),
                               std::declval<AddTwoInts::Response&>(), 10u)),
                           ::nros::Result>::value,
              "Client<Svc>::call_polling(req, resp, timeout_ms) must return nros::Result");

} // namespace nros_cpp_service_client_call_polling_compile_test
