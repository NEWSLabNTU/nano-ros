// Compile regression for issue 0089 gap-4: typed `nros::bind_service<Svc, C, &m>`.
//
// The header `-fsyntax-only` loop in `just check cpp` only PARSES the templates;
// it does not instantiate them. This TU instantiates `bind_service` against a
// service type matching the generated shape (`struct Svc { Request; Response;
// TYPE_NAME }` with `ffi_{,de}serialize`), so the template BODY is type-checked.
// `just check cpp` compiles this with `-fsyntax-only -std=c++14`.
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
    ::nros::Result configure(::rclcpp::Node& node) {
        return ::nros::bind_service<AddTwoInts, Server, &Server::on_add>(node, "/add_two_ints",
                                                                         this);
    }
};

// Force template instantiation (body type-checked at compile).
inline ::nros::Result instantiate(::rclcpp::Node& node, Server* s) {
    return s->configure(node);
}

// --- phase-456 W3: a REGISTERED callback-style service is movable ------------
//
// `service.hpp`'s move constructor used to carry a warning ("A callback-style
// service must NOT be moved after register -- the arena holds `this` as the
// trampoline context"), and W3 removed the warning by removing its subject: the
// arena's context is the user's HANDLER now, so nothing of the caller's is
// referenced after registration.
//
// A deleted comment is not evidence. This is: the move below is the operation
// the warning forbade, performed on an object that has been through the
// callback-style `create_service` / `create_client`. If anyone re-registers
// `&out` as the context, the code here still compiles — so the `static_assert`s
// pin the TYPE property and the moves pin the shape, and the thing that would
// actually catch a regression is that this TU documents which call it is.
// `AddTwoInts` above is the minimal shape `bind_service` needs; the typed
// service/client templates also read `SERIALIZED_SIZE_MAX` and `TYPE_HASH`, so
// this is the full codegen shape rather than an edit to the stub the existing
// probe is written against.
struct AddTwoIntsFull {
    struct Request {
        int64_t a{0};
        int64_t b{0};
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
        int64_t sum{0};
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
    static constexpr const char* TYPE_HASH = "RIHS01_srv_stub";
};

void w3_add_two_ints(const AddTwoIntsFull::Request& req, AddTwoIntsFull::Response& resp) {
    resp.sum = req.a + req.b;
}
void w3_on_response(const AddTwoIntsFull::Response& resp) {
    (void)resp;
}

static_assert(__is_constructible(::nros::Service<AddTwoIntsFull>,
                                 ::nros::Service<AddTwoIntsFull>&&),
              "phase-456 W3: a callback-style service is movable -- the arena holds the handler, "
              "not `&out`");
static_assert(__is_constructible(::nros::Client<AddTwoIntsFull>, ::nros::Client<AddTwoIntsFull>&&),
              "phase-456 W3: a callback-style client is movable -- the arena holds the handler, "
              "not `&out`");

inline ::nros::Result w3_move_after_register(::rclcpp::Node& node) {
    ::nros::Service<AddTwoIntsFull> srv;
    ::nros::Result r =
        node.create_service<AddTwoIntsFull>(srv, "/add_two_ints_cb", &w3_add_two_ints);
    if (!r.ok()) return r;
    // THE OPERATION THE OLD WARNING FORBADE.
    ::nros::Service<AddTwoIntsFull> moved_srv(static_cast<::nros::Service<AddTwoIntsFull>&&>(srv));
    ::nros::Service<AddTwoIntsFull> assigned_srv;
    assigned_srv = static_cast<::nros::Service<AddTwoIntsFull>&&>(moved_srv);

    ::nros::Client<AddTwoIntsFull> cli;
    r = node.create_client<AddTwoIntsFull>(cli, "/add_two_ints_cb", &w3_on_response);
    if (!r.ok()) return r;
    ::nros::Client<AddTwoIntsFull> moved_cli(static_cast<::nros::Client<AddTwoIntsFull>&&>(cli));
    // The one verb the corpus measured on a dispatch client, reached THROUGH the
    // move: `async_send_request` needs `{executor_, handle_id_}` and nothing
    // else, which is exactly what the move carries.
    AddTwoIntsFull::Request req;
    return moved_cli.async_send_request(req);
}

} // namespace nros_cpp_bind_service_compile_test
