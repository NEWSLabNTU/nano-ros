// Compile regression for phase-417 stage 1 (RFC-0089) — the three "cheap
// unblockers" that the first lines of nearly every ported rclcpp file need.
//
// W1.a  nested `SharedPtr` / `ConstSharedPtr` / `UniquePtr` on the entity
//       types and on `rclcpp::Timer`, so
//       `rclcpp::Publisher<T>::SharedPtr member_;` — close to universal in
//       real rclcpp source — declares.
// W1.c  `std::string` interop on `nros::FixedString<N>` / `nros::HeapString`,
//       so `message.data = "Hello, world! " + std::to_string(n);` compiles
//       against a codegen'd string field.
// W1.d  `now()` / `get_clock()` / `get_name()` / `get_namespace()` on the shim
//       `rclcpp::Node`, plus the `rclcpp::Time` / `Duration` / `Clock`
//       aliases.
//
// None of this is behaviour — RFC-0089 §"Who implements an adopted name"
// allows aliases, forwarders and copying conversions in the wrapper and
// nothing else. The header `-fsyntax-only` loop in `just check cpp` only
// PARSES templates, so a nested alias inside a class template is never
// checked there; this TU instantiates them.
//
// phase-417 stage 6 step A — this reaches the surface through `<nros/nros.hpp>`
// and no longer through `nros/rclcpp_compat.hpp`, which now declares nothing.
// The include is the POINT of the probe as much as the body is: the `rclcpp::`
// names are declared by the API headers themselves, so a probe that still went
// through the shim would pass whether or not the move had happened.
//
// Compiled HOSTED (no `-ffreestanding`): `std::shared_ptr` is in the public
// signature of every `rclcpp::Node::create_*`. The freestanding half of the
// contract — that those names VANISH rather than break the build where
// `<memory>` is absent — is covered by the header loop itself, which parses
// every header including `nros.hpp` with `-ffreestanding`.

#include <nros/nros.hpp>

// The string containers codegen emits for a message field. Neither is reachable
// from the `nros.hpp` umbrella today (a separate stage-2 gap: 15 of 46 headers
// are not), so name them directly.
#include <nros/fixed_string.hpp>
#include <nros/heap_string.hpp>

#include <memory>
#include <string>
#include <type_traits>

namespace nros_cpp_ros2_api_adoption_compile_test {

// Mirror of a codegen'd message with a FIXED-capacity string field
// (`mode = "fixed"`, the default) — cf. std_msgs/msg/String.
struct StringMsg {
    ::nros::FixedString<256> data;
    static const size_t SERIALIZED_SIZE_MAX = 512;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::String_";
    static constexpr const char* TYPE_HASH = "RIHS01_string_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

// The same message with a HEAP string field (`mode = "heap"`, RFC-0033).
struct HeapStringMsg {
    ::nros::HeapString data;
    static const size_t SERIALIZED_SIZE_MAX = 512;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::String_";
    static constexpr const char* TYPE_HASH = "RIHS01_string_stub";
    static int ffi_publish(void*, const void*) { return 0; }
};

// --- W1.a + W1.d: the upstream node shape, written the upstream way ---------
//
// Every member declaration here is the spelling the ROS 2 tutorial uses. The
// point of the class is that NONE of them needs a `std::shared_ptr<…>`
// rewrite.
class PortedNode : public rclcpp::Node {
  public:
    PortedNode() : rclcpp::Node("ported_node") {
        publisher_ = this->create_publisher<StringMsg>("topic", 10);
        timer_ = this->create_wall_timer(std::chrono::milliseconds(500), [this]() { tick(); });

        // W1.d — identity and clock, forwarded to `rclcpp::Node`.
        const char* name = this->get_name();
        const char* ns = this->get_namespace();
        rclcpp::Time stamp = this->now();
        rclcpp::Clock* clock = this->get_clock();
        rclcpp::Time via_clock = clock->now();
        (void)name;
        (void)ns;
        (void)stamp;
        (void)via_clock;
    }

  private:
    void tick() {
        StringMsg message;
        // W1.c — the upstream line, verbatim. Before this stage the only
        // assignment `FixedString<N>` had was from `const char*`, so a ported
        // file had to insert a `.c_str()`.
        message.data = "Hello, world! " + std::to_string(count_++);

        // …and back out again, both spellings.
        const std::string round_trip = message.data;
        const std::string explicit_round_trip = message.data.to_string();
        bool eq = message.data == round_trip;
        bool ne = message.data != std::string("something else");
        bool eq_cstr = message.data == "Hello, world! 0";
        (void)round_trip;
        (void)explicit_round_trip;
        (void)eq;
        (void)ne;
        (void)eq_cstr;

        // Ungated `std::string`-shaped queries.
        size_t n = message.data.size();
        bool is_empty = message.data.empty();
        (void)n;
        (void)is_empty;

        // `(void)`, where upstream's file writes the bare call: ours
        // WIDENS the return type — `rclcpp::Publisher::publish` returns
        // void, `nros::Publisher::publish` returns `Result` — and
        // phase-427 W8 put `NROS_NODISCARD` on that type. So a ported
        // file has to say what it wants done with the failure. That is
        // the porting cost the attribute exists to charge, made visible
        // here rather than left as a silent drop.
        (void)publisher_->publish(message);
    }

    // W1.a — the nested-pointer spellings, as members.
    rclcpp::Timer::SharedPtr timer_;
    rclcpp::Publisher<StringMsg>::SharedPtr publisher_;
    size_t count_ = 0;
};

// --- W1.a: every entity type carries the three aliases ----------------------

struct StubService {
    struct Request {
        static const size_t SERIALIZED_SIZE_MAX = 16;
    };
    struct Response {
        static const size_t SERIALIZED_SIZE_MAX = 16;
    };
    static constexpr const char* TYPE_NAME = "example_interfaces::srv::dds_::AddTwoInts_";
    static constexpr const char* TYPE_HASH = "RIHS01_srv_stub";
};

// phase-456 W5 — `Publisher<M>::SharedPtr` is DELIBERATELY not a
// `std::shared_ptr`, and these three assertions inverted to say so.
//
// It is `nros::Owned<Publisher<M>>`: the publisher BY VALUE, move-only, with an
// `operator->` so every `pub_->publish(m)` in the porting corpus (41 sites)
// keeps working, and with no allocator, control block or `<memory>` — so the
// alias exists on a freestanding target, which is the one thing
// `std::shared_ptr` could not do.
//
// NOT an arena handle, unlike `Subscription<M>::SharedPtr` below, and W4
// measured why: the arena is a bump allocator with no removal path, so an arena
// publisher would make `reset()` and scope exit no-ops over a live RMW
// publisher. The Rust side returns `EmbeddedPublisher<M>` BY VALUE with a
// `Drop`, and `Owned<T>` mirrors that lifetime exactly.
static_assert(std::is_same<::nros::Publisher<StringMsg>::SharedPtr,
                           ::nros::Owned<::nros::Publisher<StringMsg>>>::value,
              "Publisher<M>::SharedPtr must be nros::Owned<Publisher<M>>");
// `ConstSharedPtr` and `UniquePtr` are the SAME TYPE, and each for its own
// reason. `Owned<const T>` is a hard error (`owned.hpp` says so and names the
// resolution: a const VIEW is `const Owned<T>&`, because a const/mutable
// distinction over a handle presupposes shared ownership a sole owner has not
// got). And `Owned<T>` already IS unique ownership, so a separate unique alias
// would be a second spelling of one type — measured zero uses in the tree
// before the collapse.
static_assert(std::is_same<::nros::Publisher<StringMsg>::ConstSharedPtr,
                           ::nros::Publisher<StringMsg>::SharedPtr>::value,
              "Publisher<M>::ConstSharedPtr is the SAME type as SharedPtr");
static_assert(std::is_same<::nros::Publisher<StringMsg>::UniquePtr,
                           ::nros::Publisher<StringMsg>::SharedPtr>::value,
              "Publisher<M>::UniquePtr is the SAME type as SharedPtr -- Owned<T> IS unique "
              "ownership, so a second alias would be a second spelling of one type");
// The publisher handle DEREFERENCES, which is the property that separates it
// from the two arena handles below. A ported body calls `publish` on it 41
// times across the corpus; a two-word arena handle deliberately has no
// `operator->`, which is why a publisher could not take that shape.
static_assert(
    std::is_same<decltype(std::declval<::nros::Publisher<StringMsg>::SharedPtr&>().operator->()),
                 ::nros::Publisher<StringMsg>*>::value,
    "Publisher<M>::SharedPtr must dereference to the publisher -- the corpus calls publish() "
    "through it");
// phase-456 W2 — `Subscription<M>::SharedPtr` is DELIBERATELY not a
// `std::shared_ptr`, and this assertion inverted to say so.
//
// A subscription created with a callback is owned by the Rust executor arena,
// which holds the subscriber, the rx buffer, the callback and its capture.
// There is no C++ object to point at. What the factory used to hand back was a
// `shared_ptr` aliasing into a heap cell, carrying `take()`,
// `take_serialized()`, `take_validated()`, `take_sequence()` and `borrow()` --
// every one of them a call into 656 zero bytes the arena never filled.
//
// phase-456 W2b then moved that API to `nros::PollSubscription<M>`, which is
// the type that owns a subscriber, and REPAID W2's stated cost: the handle can
// name `Subscription<M>` as its `element_type` again, because every operation
// left on that class is one an arena registration can perform.
static_assert(std::is_same<::nros::Subscription<StringMsg>::SharedPtr,
                           ::nros::SubscriptionHandle<StringMsg>>::value,
              "Subscription<M>::SharedPtr must be the two-word arena handle");
static_assert(sizeof(::nros::Subscription<StringMsg>::SharedPtr) == 2 * sizeof(void*),
              "the dispatch handle must stay two words -- it is what replaced a 984-byte "
              "object plus a heap cell (measured phase-456 W2b; the dispatch class is "
              "304 bytes now and the poll one carries the storage)");
static_assert(std::is_same<::nros::Subscription<StringMsg>::ConstSharedPtr,
                           ::nros::Subscription<StringMsg>::SharedPtr>::value,
              "ConstSharedPtr is the same handle: there is no const/mutable distinction to "
              "draw over a registration that exposes no operation on the entity");
static_assert(std::is_same<::nros::Subscription<StringMsg>::SharedPtr::element_type,
                           ::nros::Subscription<StringMsg>>::value,
              "phase-456 W2b: SharedPtr::element_type names the dispatch subscription again");

// And the half that makes that name honest: the taking API is NOT reachable
// through `Subscription<M>`. A REACHABILITY assertion, in the shape
// `ros2_one_dispatch_path.cpp` uses for `Node::pump()` -- the method coming
// back under any signature is what this has to catch, because a `take()` on
// this class compiles, dispatches into an unfilled `RmwSubscriber`, and
// reports nothing.
template <typename T, typename = void> struct has_take : std::false_type {};
template <typename T>
struct has_take<T, decltype(void(std::declval<T&>().take(std::declval<StringMsg&>())))>
    : std::true_type {};
static_assert(!has_take<::nros::Subscription<StringMsg>>::value,
              "phase-456 W2b: the dispatch subscription must not carry a take() -- the arena "
              "owns the subscriber and this object has no storage to take from");
static_assert(has_take<::nros::PollSubscription<StringMsg>>::value,
              "phase-456 W2b: the poll subscription is where take() went");
static_assert(std::is_same<::nros::PollingSubscription<StringMsg>::SharedPtr,
                           ::nros::Owned<::nros::PollingSubscription<StringMsg>>>::value,
              "PollingSubscription<M>::SharedPtr must be nros::Owned<PollingSubscription<M>> -- "
              "the caller owns the subscriber, so the holder owns the object");
// phase-456 W5 — `Service<S>::SharedPtr` is the arena handle, for the same
// reason the subscription's is, and W3 measured the same finding one entity
// over: across `examples/`, `tests/`, `book/` and `packages/`, NOTHING is
// invoked on a dispatch service. It is stored and dropped.
//
// The blocker W3 recorded was the alias serving two owners:
// `create_service<S>(name)` with no handler also returned it and existed to be
// `->take_request()`'d. That poll half is `nros::PollService<S>` now, which is
// what unblocks this line.
static_assert(std::is_same<::nros::Service<StubService>::SharedPtr,
                           ::nros::ServiceHandle<StubService>>::value,
              "Service<S>::SharedPtr must be the two-word arena handle");
static_assert(sizeof(::nros::Service<StubService>::SharedPtr) == 2 * sizeof(void*),
              "the dispatch service handle must stay two words");
static_assert(std::is_same<::nros::Service<StubService>::ConstSharedPtr,
                           ::nros::Service<StubService>::SharedPtr>::value,
              "ConstSharedPtr is the same handle: there is no const/mutable distinction to "
              "draw over a registration that exposes no operation on the entity");
static_assert(std::is_same<::nros::Service<StubService>::SharedPtr::element_type,
                           ::nros::Service<StubService>>::value,
              "ServiceHandle<S>::element_type names the dispatch service");
// And the half that makes that name honest: the taking API is NOT reachable
// through `Service<S>`. Same REACHABILITY shape as `has_take` above, and for
// the same measured reason — `take_request` on a dispatch service used to hand
// NROS_SERVICE_SERVER_SIZE zero bytes to the FFI, with `initialized_` true and
// nothing on the path checking.
template <typename T, typename = void> struct has_take_request : std::false_type {};
template <typename T>
struct has_take_request<T,
                        decltype(void(std::declval<T&>().take_request(
                            std::declval<typename T::RequestType&>(), std::declval<int64_t&>())))>
    : std::true_type {};
static_assert(!has_take_request<::nros::Service<StubService>>::value,
              "phase-456 W5: the dispatch service must not carry take_request() -- the arena "
              "owns the server and this object has no storage to take from");
static_assert(has_take_request<::nros::PollService<StubService>>::value,
              "phase-456 W5: the poll service is where take_request() went");
static_assert(std::is_same<::nros::Client<StubService>::SharedPtr,
                           std::shared_ptr<::nros::Client<StubService>>>::value,
              "Client<S>::SharedPtr is still std::shared_ptr<Client<S>> -- phase-456 W5 did "
              "NOT flip it. W3 measured the shape a ClientHandle<S> would take (two words "
              "plus async_send_request), but the FUTURE-style create_client<S>(name, qos) is "
              "the same collision the poll create_service was, and splitting a PollClient<S> "
              "out is its own item");
static_assert(std::is_same<::nros::Timer::SharedPtr, std::shared_ptr<::nros::Timer>>::value,
              "Timer::SharedPtr must be std::shared_ptr<Timer>");
static_assert(std::is_same<rclcpp::Timer::SharedPtr, std::shared_ptr<rclcpp::Timer>>::value,
              "Timer::SharedPtr must be std::shared_ptr<Timer>");
static_assert(std::is_same<rclcpp::Timer::UniquePtr, std::unique_ptr<rclcpp::Timer>>::value,
              "Timer::UniquePtr must be std::unique_ptr<Timer>");

// The rclcpp alias templates hand the SAME nested names through.
static_assert(std::is_same<rclcpp::Publisher<StringMsg>::SharedPtr,
                           ::nros::Publisher<StringMsg>::SharedPtr>::value,
              "rclcpp::Publisher<M>::SharedPtr must resolve through the nros:: alias");
static_assert(std::is_same<rclcpp::Subscription<StringMsg>::SharedPtr,
                           ::nros::Subscription<StringMsg>::SharedPtr>::value,
              "rclcpp::Subscription<M>::SharedPtr must resolve through the nros:: alias");
static_assert(std::is_same<rclcpp::Service<StubService>::SharedPtr,
                           ::nros::Service<StubService>::SharedPtr>::value,
              "rclcpp::Service<S>::SharedPtr must resolve through the nros:: alias");
static_assert(std::is_same<rclcpp::Client<StubService>::SharedPtr,
                           ::nros::Client<StubService>::SharedPtr>::value,
              "rclcpp::Client<S>::SharedPtr must resolve through the nros:: alias");

// --- W1.d: the clock vocabulary is the nano-ros type, not a wrapper ---------

static_assert(std::is_same<rclcpp::Time, ::nros::Time>::value,
              "rclcpp::Time must BE nros::Time — a second type is a second contract");
static_assert(std::is_same<rclcpp::Duration, ::nros::Duration>::value,
              "rclcpp::Duration must BE nros::Duration");
static_assert(std::is_same<rclcpp::Clock, ::nros::Clock>::value,
              "rclcpp::Clock must BE nros::Clock");

// The shim's accessors have the shapes `rclcpp::Node` has, since they forward.
static_assert(
    std::is_same<decltype(std::declval<const rclcpp::Node&>().get_name()), const char*>::value,
    "rclcpp::Node::get_name() must return const char*, as upstream does");
static_assert(
    std::is_same<decltype(std::declval<const rclcpp::Node&>().get_namespace()), const char*>::value,
    "rclcpp::Node::get_namespace() must return const char*, as upstream does");
static_assert(
    std::is_same<decltype(std::declval<const rclcpp::Node&>().now()), ::nros::Time>::value,
    "rclcpp::Node::now() must return nros::Time");
static_assert(
    std::is_same<decltype(std::declval<rclcpp::Node&>().get_clock()), ::nros::Clock*>::value,
    "rclcpp::Node::get_clock() must return a borrowed nros::Clock*");

// --- W1.c: HeapString carries the same conversions as FixedString -----------

inline void heap_string_round_trip(HeapStringMsg& msg) {
    msg.data = std::string("Hello, world! ") + std::to_string(1);
    msg.data = "plain C string";
    const std::string out = msg.data;
    const std::string explicit_out = msg.data.to_string();
    bool eq = msg.data == out;
    bool eq_cstr = msg.data == "plain C string";
    bool ne = msg.data != std::string("other");
    (void)out;
    (void)explicit_out;
    (void)eq;
    (void)eq_cstr;
    (void)ne;
}

// --- The layout the FFI depends on is unchanged (RFC-0033 / heap_string.hpp) -
//
// W1.c adds member FUNCTIONS only. A data member would silently break every
// generated message struct that embeds one of these.
static_assert(sizeof(::nros::FixedString<256>) == 256,
              "FixedString<N> must stay layout-identical to char[N]");
static_assert(sizeof(::nros::FixedString<8>) == 8,
              "FixedString<N> must stay layout-identical to char[N]");
static_assert(sizeof(::nros::HeapString) == sizeof(char*) + 2 * sizeof(size_t),
              "HeapString must stay { char* data; size_t size; size_t capacity; }");

// Instantiate the ported node's members so the bodies above are type-checked.
inline void instantiate() {
    // phase-456 W5 — this line used to read
    // `= std::make_shared<::nros::Publisher<StringMsg>>()`. There is no
    // `make_shared` for a publisher any more, and there is nothing to allocate:
    // the entity IS the member. `= nullptr` is the ported spelling
    // `Owned<T>` exists to accept, and the publisher moves in later from
    // `create_publisher`, exactly as `owned_publisher_ported_shape.cpp` shows.
    ::nros::Publisher<StringMsg>::SharedPtr pub = nullptr;
    rclcpp::Timer::SharedPtr timer;
    (void)pub;
    (void)timer;
}

} // namespace nros_cpp_ros2_api_adoption_compile_test
