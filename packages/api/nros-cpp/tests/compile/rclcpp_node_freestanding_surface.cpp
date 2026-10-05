// phase-438 W4 — `rclcpp::Node`'s UNCONDITIONAL surface, instantiated.
//
// phase-427 W1-W3/W5 made `rclcpp::Node` one class (`= ::rclcpp::Node`) with a
// fixed layout on every target, and made the std-flavoured factories ADDITIVE
// overloads beside out-ref forms that mirror `rclcpp::Node`.
// `check-cpp-capability-layout` measures the layout half; nothing measured the
// method half, and it could not have:
//
//   * the per-header `-fsyntax-only` loop in `just check cpp` only PARSES the
//     templates, and an uninstantiated template body is barely checked;
//   * every other ported-surface probe compiles with `-DNROS_CPP_STD=1`, which
//     is the configuration where the new overloads are the LEAST interesting.
//
// So the out-ref overloads would have been a claim: declared, never compiled,
// and green either way. That is `check-required-features-reachable`'s class one
// directory over — a target nobody builds reads as coverage.
//
// This TU is compiled by the `cpp` lane in the two configurations where the
// claim actually says something: hosted WITHOUT `-DNROS_CPP_STD`, and
// `-nostdinc++` against the ThreadX shim. Both have to work, because since
// phase-438 W2 the first is what an ordinary hosted consumer gets and the
// second is what an embedded one gets.
//
// It deliberately names NOTHING from the porting surface — no `std::string`,
// no `std::shared_ptr`, no `Node::SharedPtr`, no `rclcpp::spin(node)`. If a
// later change moves one of those onto the unconditional half it will not
// break this file; if a change moves something OFF the unconditional half, this
// file stops compiling, which is the direction that matters.
//
// WHERE THE LINE ACTUALLY FALLS, measured against `node.hpp` rather than
// assumed (before phase-476 W3, which split it). The hosted-node conjunction gated more than the
// `shared_ptr` factories: `initialized()`, `get_node_options()`, `parameters()` and the WHOLE
// parameter facade — the `const char*`-keyed forms included — are hosted-only,
// because the store they read (`hosted().params`) and the options object
// (`hosted().options`) both live in the lazily-allocated hosted box. So this
// file uses `ok()`, which is the unconditional answer to the same question as
// `initialized()`, and does not name the parameter forwarders at all. Adding
// one here would be asserting a contract the class does not offer.
#include <nros/nros.hpp>

namespace nros_cpp_rclcpp_node_freestanding_compile_test {

// Mirror of a codegen'd message (cf. std_msgs/msg/Int32).
struct Int32 {
    int32_t data{0};
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

// Mirror of a codegen'd service binding (cf. example_interfaces/srv/AddTwoInts).
struct AddTwoInts {
    struct Request {
        int64_t a{0};
        int64_t b{0};
        static const size_t SERIALIZED_SIZE_MAX = 32;
        static constexpr const char* TYPE_HASH = "RIHS01_add_two_ints_request_stub";
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
        static const size_t SERIALIZED_SIZE_MAX = 32;
        static constexpr const char* TYPE_HASH = "RIHS01_add_two_ints_response_stub";
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

void on_sample(const Int32&) {}
void on_request(const AddTwoInts::Request&, AddTwoInts::Response&) {}
void on_response(const AddTwoInts::Response&) {}
void on_tick(void*) {}

// A `rclcpp::Node` EXISTS here at all. Before W4 the whole class sat inside
// `#if defined(NROS_CPP_HAS_SHARED_PTR) && ...`, so this declaration was the
// first error in the file.
inline ::nros::Result instantiate() {
    rclcpp::Node node("freestanding_probe");
    (void)node.ok();
    (void)node.get_name();
    (void)node.get_namespace();
    (void)node.now();
    (void)node.get_clock();

    // The out-ref factories — caller-owned storage, `Result` channel, `const
    // char*` names. Exactly `rclcpp::Node`'s shape, which is what 27 of the 28
    // in-tree `create_*` call sites already write.
    ::nros::Publisher<Int32> pub;
    ::nros::Result r = node.create_publisher(pub, "/count", ::nros::QoS(10));

    ::nros::Subscription<Int32> sub;
    (void)node.create_subscription(sub, "/count", ::nros::QoS(10), &on_sample);

    ::nros::Timer timer;
    (void)node.create_wall_timer(timer, 100, &on_tick, nullptr);

    // phase-476 W2 — the VALUE-returning timer verbs, freestanding: a
    // capturing lambda (held in the executor arena, not a `std::function`)
    // and a `TimerHandle` (not a `std::shared_ptr`). Both spellings of reset
    // are distinct here too: `->reset()` restarts, `.reset()` releases.
    int ticks = 0;
    rclcpp::Timer::SharedPtr wall = node.create_wall_timer(
        ::nros::Duration::from_nanoseconds(100000000), [&ticks]() { ++ticks; });
    (void)wall->cancel();
    (void)wall->reset();
    (void)wall->is_ready();
    (void)wall->time_until_trigger();
    rclcpp::TimerBase::SharedPtr on_clock =
        rclcpp::create_timer(node, node.get_clock(), ::nros::Duration::from_nanoseconds(100000000),
                             [&ticks]() { ++ticks; });
    on_clock.reset();
    wall.reset();

    // phase-476 W1 — the value-returning DISPATCH verbs with a `const char*`
    // key, freestanding: a capturing lambda for the subscription, plain
    // function pointers for the service and client (the callable those two
    // take), and the poll forms by value.
    rclcpp::Subscription<Int32>::SharedPtr vsub = node.create_subscription<Int32>(
        "/count", ::nros::QoS(10), [&ticks](const Int32&) { ++ticks; });
    rclcpp::Subscription<Int32>::SharedPtr vsub_depth =
        node.create_subscription<Int32>("/count", 10, [&ticks](const Int32&) { ++ticks; });
    rclcpp::Service<AddTwoInts>::SharedPtr vsrv =
        node.create_service<AddTwoInts>("/add_v", &on_request);
    rclcpp::Client<AddTwoInts>::SharedPtr vcli =
        node.create_client<AddTwoInts>("/add_v", &on_response);
    auto vpoll_srv = node.create_service<AddTwoInts>("/add_poll");
    auto vpoll_cli = node.create_client<AddTwoInts>("/add_poll");
    (void)vsub;
    (void)vsub_depth;
    (void)vsrv;
    (void)vcli;
    (void)vpoll_srv;
    (void)vpoll_cli;

    // phase-456 W5 — the poll server is `nros::PollService<S>`, a different
    // type from the dispatch one below. One class used to be both.
    ::nros::PollService<AddTwoInts> poll_service;
    (void)node.create_service<AddTwoInts>(poll_service, "/add");
    ::nros::Service<AddTwoInts> cb_service;
    (void)node.create_service<AddTwoInts>(cb_service, "/add_cb", &on_request);

    // phase-456 W9 — the future-style client is `nros::PollClient<S>`, a
    // different type from the dispatch one below, for the reason the service
    // pair above states: one class used to be both, and every verb that reads
    // caller storage was offered on the half that has none.
    ::nros::PollClient<AddTwoInts> future_client;
    (void)node.create_client<AddTwoInts>(future_client, "/add");
    ::nros::Client<AddTwoInts> cb_client;
    (void)node.create_client<AddTwoInts>(cb_client, "/add_cb", &on_response);

    // 2026-09-28 (the phase-456 W9 gap closure) — the two DISCOVERY verbs on the
    // DISPATCH road, instantiated in a freestanding TU because that is where the
    // surface has to exist: both are template bodies, so the header sweep parses
    // them and type-checks neither. `{executor_, handle_id_}` is the whole
    // argument list, which is what makes them reachable here at all.
    (void)cb_client.service_is_ready();
    (void)cb_client.wait_for_service(1000);
    // ...and on the HANDLE a porter actually holds (`Client<S>::SharedPtr`).
    ::nros::ClientHandle<AddTwoInts> cli_handle = nullptr;
    (void)cli_handle.service_is_ready();
    (void)cli_handle.wait_for_service(1000);

    // The node is CONSTRUCTED by value above; a freestanding target has no
    // `std::make_shared` to reach for, and does not need one.
    return r;
}

// Deriving costs nothing freestanding — no allocator, no exceptions, and no
// vtable, because `rclcpp::Node` has no virtual member. RFC-0089's correction 1
// measured this and this is where it stays measured: phase-427 keeps ROS 2's
// shape, which means keeping DERIVATION.
class Derived : public rclcpp::Node {
  public:
    Derived() : rclcpp::Node("derived_probe") {}
    ::nros::Result start() { return create_wall_timer(timer_, 1000, &on_tick, this); }

  private:
    ::nros::Timer timer_;
};

static_assert(!__is_polymorphic(rclcpp::Node), "rclcpp::Node must introduce no vtable");
static_assert(!__is_polymorphic(Derived), "deriving from rclcpp::Node must introduce no vtable");

} // namespace nros_cpp_rclcpp_node_freestanding_compile_test
