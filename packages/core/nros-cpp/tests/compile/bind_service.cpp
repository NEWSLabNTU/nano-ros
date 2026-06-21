// Compile regression for issue 0089 gap-4: typed `nros::bind_service<Svc, C, &m>`.
//
// The header `-fsyntax-only` loop in `just check-cpp` only PARSES the templates;
// it does not instantiate them. This TU instantiates `bind_service` against a
// service type matching the generated shape (`struct Svc { Request; Response;
// TYPE_NAME }` with `ffi_{,de}serialize`), so the template BODY is type-checked.
// `just check-cpp` compiles this with `-fsyntax-only -std=c++14`.
#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace nros_cpp_bind_service_compile_test {

// Mirror of a codegen'd service binding (cf. example_interfaces/srv/AddTwoInts).
struct AddTwoInts {
    struct Request {
        int64_t a{0};
        int64_t b{0};
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
        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Response_";
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
            if (out) *out = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };
    static constexpr const char* TYPE_NAME = "example_interfaces::srv::dds_::AddTwoInts_";
};

class Server {
  public:
    // Typed handler — `Response on_add(const Request&)`; no hand-rolled CDR.
    AddTwoInts::Response on_add(const AddTwoInts::Request& req) {
        AddTwoInts::Response resp;
        resp.sum = req.a + req.b;
        return resp;
    }
    ::nros::Result configure(::nros::Node& node) {
        return ::nros::bind_service<AddTwoInts, Server, &Server::on_add>(node, "/add_two_ints",
                                                                         this);
    }
};

// Force template instantiation (body type-checked at compile).
inline ::nros::Result instantiate(::nros::Node& node, Server* s) {
    return s->configure(node);
}

} // namespace nros_cpp_bind_service_compile_test
