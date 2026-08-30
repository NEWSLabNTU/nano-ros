// Compile regression for issue 0278: `nros::PollingSubscription<M>` — the
// latest-value polling subscriber (analog of
// `autoware_utils::InterProcessPollingSubscriber`).
//
// The header `-fsyntax-only` loop in `just check cpp` only PARSES the templates;
// it does not instantiate them. This TU instantiates `PollingSubscription`
// against a message type matching the generated shape (`SERIALIZED_SIZE_MAX`,
// `TYPE_NAME`, `TYPE_HASH`, `ffi_deserialize`), so the wrapper BODY
// (`drain`, `take_data`, `take_new_data`, `take`, `peek`) is type-checked and
// the `Node::create_polling_subscription` factory path is exercised.
// `just check cpp` compiles this with `-fsyntax-only -std=c++14`.
#include <nros/nros.hpp>

#include <type_traits>

namespace nros_cpp_polling_subscription_compile_test {

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

// Force template instantiation (bodies type-checked at compile).
inline ::nros::Result instantiate(::nros::Node& node) {
    ::nros::PollingSubscription<Int32> sub;
    ::nros::Result r = node.create_polling_subscription(sub, "/count");

    // The latest-value accessors: `take_data`/`take_new_data`/`peek` return a
    // `const Int32*`; `take` copies into an out-param and returns a bool;
    // `has_data`/`is_valid` are bool.
    const Int32* latest = sub.take_data();
    const Int32* fresh = sub.take_new_data();
    const Int32* peeked = sub.peek();
    Int32 out{};
    bool got = sub.take(out);
    bool has = sub.has_data();
    bool valid = sub.is_valid();

    (void)latest;
    (void)fresh;
    (void)peeked;
    (void)got;
    (void)has;
    (void)valid;
    return r;
}

// API-shape static assertions — a regression in the return types stops
// compiling here (same intent as spin_verbs.cpp).
static_assert(
    std::is_same<decltype(std::declval<::nros::PollingSubscription<Int32>&>().take_data()),
                 const Int32*>::value,
    "PollingSubscription<M>::take_data() must return const M*");
static_assert(
    std::is_same<decltype(std::declval<::nros::PollingSubscription<Int32>&>().take_new_data()),
                 const Int32*>::value,
    "PollingSubscription<M>::take_new_data() must return const M*");
static_assert(std::is_same<decltype(std::declval<::nros::PollingSubscription<Int32>&>().take(
                               std::declval<Int32&>())),
                           bool>::value,
              "PollingSubscription<M>::take(M&) must return bool");

} // namespace nros_cpp_polling_subscription_compile_test
