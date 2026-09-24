// phase-444 — every C++ entity class reads back the name it was created on.
//
// WHY THIS PROBE EXISTS. The accessor family was complete for four entity
// classes and absent for two: `Publisher::get_topic_name` and
// `Subscription::get_topic_name` shipped from the start, both action tiers
// gained `get_action_name()` in phase-417 W4.b, and the SERVICE pair — ledger
// rows `cpp:Client::get_service_name` and `cpp:Service::get_service_name` — had
// nothing in any tier. A gap like that is invisible to every runtime test,
// because the thing that regressed is a name a ported file cannot spell; so the
// question is a compile-time one and it gets a compile-time answer.
//
// It is written as ONE list on purpose. The defect this family has actually had
// is per-class drift — phase-417 W4.b found `get_action_name()` on two of the
// four action classes and called it "four surfaces, one accessor" — and a probe
// per class is how that goes unnoticed again. Adding an entity class without
// adding its row here is the omission to catch.
//
// METHOD POINTERS, as `node_graph_forwarders.cpp` does: a missing method, a
// return type that drifted, or an accessor that quietly lost its `const` fails
// to compile here rather than at a user's build. There is no runtime, no
// session and no node, and none is needed — every one of these reads a member
// the class already owns.
//
// The pointers are taken through a `const`-qualified member type because a
// ported file often holds a `const Publisher &`, which is upstream's shape
// (`rclcpp::PublisherBase::get_topic_name() const`).
//
// `just check cpp` compiles this with `-fsyntax-only -std=c++14 -DNROS_CPP_STD=1`.

#include <nros/nros.hpp>
#include <nros/polling_action_client.hpp>
#include <nros/polling_action_server.hpp>

#include <stddef.h>
#include <stdint.h>

namespace nros_cpp_entity_name_accessors_compile_test {

// Mirror of a codegen'd message.
struct Payload {
    int32_t value{0};
    static const size_t SERIALIZED_SIZE_MAX = 32;
    static constexpr const char* TYPE_NAME = "test_msgs::msg::dds_::Payload_";
    static constexpr const char* TYPE_HASH = "RIHS01_payload_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_deserialize(const uint8_t*, size_t, Payload*) { return 0; }
    static int ffi_serialize(const Payload*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

// Mirror of a codegen'd service type.
struct StubService {
    struct Request {
        static const size_t SERIALIZED_SIZE_MAX = 16;
        static constexpr const char* TYPE_NAME = "test_msgs::srv::dds_::Stub_Request_";
        static constexpr const char* TYPE_HASH = "RIHS01_srv_stub";
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
            if (out) *out = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };
    struct Response {
        static const size_t SERIALIZED_SIZE_MAX = 16;
        static constexpr const char* TYPE_NAME = "test_msgs::srv::dds_::Stub_Response_";
        static constexpr const char* TYPE_HASH = "RIHS01_srv_stub";
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
            if (out) *out = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };
    static constexpr const char* TYPE_NAME = "test_msgs::srv::dds_::Stub_";
    static constexpr const char* TYPE_HASH = "RIHS01_srv_stub";
};

// Mirror of a codegen'd action type.
struct StubAction {
    using Goal = Payload;
    using Result = Payload;
    using Feedback = Payload;
    static constexpr const char* TYPE_NAME = "test_msgs::action::dds_::Stub_";
};

// ── The list. One row per entity class that remembers its own name. ────────
//
// Six families, NINE classes: publisher, subscription, the service client and
// its TWO servers (phase-444 closed the client and the server; phase-456 W5
// split the server into a dispatch half and a poll half, and both answer), and
// the action client and server in BOTH the callback tier and the polling tier.

using TopicNameFn = const char* (::rclcpp::Publisher<Payload>::*)() const;
TopicNameFn publisher_topic_name = &::rclcpp::Publisher<Payload>::get_topic_name;

using SubTopicNameFn = const char* (::rclcpp::Subscription<Payload>::*)() const;
SubTopicNameFn subscription_topic_name = &::rclcpp::Subscription<Payload>::get_topic_name;

using ClientServiceNameFn = const char* (::rclcpp::Client<StubService>::*)() const;
ClientServiceNameFn client_service_name = &::rclcpp::Client<StubService>::get_service_name;

using ServiceServiceNameFn = const char* (::rclcpp::Service<StubService>::*)() const;
ServiceServiceNameFn service_service_name = &::rclcpp::Service<StubService>::get_service_name;

// phase-456 W5 — the service server became TWO classes, so the family gained a
// ninth member and this list is where that has to show. `rclcpp::Service<S>`
// above is the DISPATCH half (the arena owns the server); `nros::PollService<S>`
// is the POLL half (the caller owns it). Each keeps its own `service_name_`,
// because the copy is made at create from the caller's argument and no FFI
// reads a name back out of either owner — so "one of them answers" would be a
// silent per-class drift of exactly the kind this probe exists to catch.
using PollServiceServiceNameFn = const char* (::nros::PollService<StubService>::*)() const;
PollServiceServiceNameFn poll_service_service_name =
    &::nros::PollService<StubService>::get_service_name;

using ActionServerNameFn = const char* (::rclcpp_action::Server<StubAction>::*)() const;
ActionServerNameFn action_server_name = &::rclcpp_action::Server<StubAction>::get_action_name;

using ActionClientNameFn = const char* (::rclcpp_action::Client<StubAction>::*)() const;
ActionClientNameFn action_client_name = &::rclcpp_action::Client<StubAction>::get_action_name;

using PollingServerNameFn = const char* (::nros::PollingActionServer<StubAction>::*)() const;
PollingServerNameFn polling_server_name = &::nros::PollingActionServer<StubAction>::get_action_name;

using PollingClientNameFn = const char* (::nros::PollingActionClient<StubAction>::*)() const;
PollingClientNameFn polling_client_name = &::nros::PollingActionClient<StubAction>::get_action_name;

// ── The MATCHED-COUNT pair, phase-444 ──────────────────────────────────────
//
// The other half of the pubsub introspection family (ledger rows
// `cpp:Publisher::get_subscription_count` and
// `cpp:Subscription::get_publisher_count`). Here as method pointers for the
// same reason the names are: the weakening is in the SIGNATURE — an executor
// and an out-parameter upstream does not take — so the signature is the thing
// that must not drift back.
//
// `const`, because upstream's is (`rclcpp::PublisherBase::
// get_subscription_count() const`) and a ported file often holds a const ref.

using PubCountFn = ::nros::Result (::rclcpp::Publisher<Payload>::*)(::nros::Executor&,
                                                                    size_t*) const;
PubCountFn publisher_subscription_count = &::rclcpp::Publisher<Payload>::get_subscription_count;

using SubCountFn = ::nros::Result (::rclcpp::Subscription<Payload>::*)(::nros::Executor&,
                                                                       size_t*) const;
SubCountFn subscription_publisher_count = &::rclcpp::Subscription<Payload>::get_publisher_count;

// ── The contract every one of them keeps ───────────────────────────────────
//
// An uninitialised entity answers `""`, NEVER NULL. A ported file writes
// `printf("%s", pub.get_topic_name())` and `%s` on NULL is undefined
// behaviour, so "no name yet" has to be a string. This is the one part of the
// contract a pointer cannot pin, so it is written out: each accessor is
// `initialized_ ? name_ : ""`.
//
// The state is reachable here — a default-constructed entity IS the
// uninitialised one, and `Node::create_*` is the only thing that leaves it
// otherwise — so assert it rather than describe it.
inline bool uninitialised_entities_answer_empty_never_null() {
    ::rclcpp::Publisher<Payload> publisher;
    ::rclcpp::Subscription<Payload> subscription;
    ::rclcpp::Client<StubService> client;
    ::rclcpp::Service<StubService> service;
    ::nros::PollService<StubService> poll_service;

    const char* names[] = {
        publisher.get_topic_name(), subscription.get_topic_name(),   client.get_service_name(),
        service.get_service_name(), poll_service.get_service_name(),
    };
    for (size_t i = 0; i < sizeof(names) / sizeof(names[0]); ++i) {
        if (names[i] == nullptr) return false;
        if (names[i][0] != '\0') return false;
    }
    return true;
}

} // namespace nros_cpp_entity_name_accessors_compile_test
