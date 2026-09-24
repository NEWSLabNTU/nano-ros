// POSITIVE probe — phase-456 W5. It used to be an EXPECTED-FAILURE.
//
// A ported `node->create_publisher<M>("chatter", 10)` must COMPILE on a
// freestanding target, and must hand back the same `Publisher<M>::SharedPtr` it
// hands back hosted. That is RFC-0096 D1 — ONE C++ API, the same shape on every
// platform — at the single most copied line in the porting corpus.
//
// WHAT THIS FILE ASSERTED BEFORE, AND WHY THE INVERSION IS THE FIX
//
// Until W5 this line was REFUSED here, and the refusal was honest given the
// signature it was refusing. The hosted overload returned
// `std::shared_ptr<Publisher<M>>`, so on a target with no allocator the only
// two answers were "fails to compile" and "compiles and allocates" — and
// RFC-0089 forbids the second, because a firmware image that silently gained a
// heap allocation per publisher would be a contract change nobody was told
// about. The file said so, and the lane's error text said so.
//
// W5 removed the premise rather than the rule. `Publisher<M>::SharedPtr` is
// `nros::Owned<Publisher<M>>` now: the publisher BY VALUE, move-only, with an
// `operator->` so `pub->publish(m)` keeps working, and with no allocator, no
// control block and no `<memory>`. W4 measured the case for it — the arena has
// no removal path, so an arena publisher would make `reset()` and scope exit
// no-ops, and the Rust `create_publisher_with_qos` returns an
// `EmbeddedPublisher<M>` by value with a `Drop` that `Owned<T>` mirrors. With
// no allocation to hide, there is nothing left to refuse, and refusing anyway
// would be a divergence this API exists to remove.
//
// The `const char*` key is the other half. A `std::string` parameter would have
// re-imposed the gate through the ARGUMENT after the return type stopped
// imposing it, so the ported overload is keyed on `const char*` (which a string
// literal binds exactly) and the `std::string` forwarders stay hosted-only.
//
// Compiled `-nostdinc++` against the ThreadX minimal libcpp. Its sibling is
// `one_node_type_freestanding.cpp`, which covers the rest of the freestanding
// node surface.

#include <nros/nros.hpp>

#include <cstddef>
#include <cstdint>

namespace nros_cpp_ported_publisher_probe {

struct CounterMsg {
    int32_t data;
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
};

// THE LINE. On a freestanding target, with no allocator and no `<memory>`.
inline void a_ported_line_compiles_here() {
    rclcpp::Node node("talker");
    auto pub = node.create_publisher<CounterMsg>("chatter", 10);
    CounterMsg msg;
    msg.data = 1;
    // `operator->` is the property that separates a publisher holder from the
    // two arena handles: the corpus calls `publish` through it 41 times.
    if (pub) (void)pub->publish(msg);
    // And `reset()` DESTROYS, which is the behaviour an arena slot could not
    // have provided — the arena's `arena_used` only grows and nothing sets an
    // entry back to `None`.
    pub.reset();
}

// The explicit-QoS spelling, which is the other one ported source writes.
inline void the_qos_spelling_compiles_too() {
    rclcpp::Node node("talker");
    rclcpp::Publisher<CounterMsg>::SharedPtr pub =
        node.create_publisher<CounterMsg>("chatter", ::nros::QoS(10));
    (void)pub;
}

// It is the SAME TYPE the hosted build returns, which is the property this
// probe exists for. A freestanding-only spelling that merely compiles would
// satisfy "it builds" and miss the point.
//
// `node_ref()` rather than `std::declval` — this TU has no `<utility>`, which
// is the whole situation being probed.
rclcpp::Node& node_ref();
static_assert(::nros::tr::is_same<decltype(node_ref().create_publisher<CounterMsg>("t", 10)),
                                  rclcpp::Publisher<CounterMsg>::SharedPtr>::value,
              "the ported create_publisher must return Publisher<M>::SharedPtr on every target");

// And the out-ref form is still there — W5 added an overload, it removed
// nothing, so a freestanding file written against the out-ref family keeps
// compiling.
inline ::nros::Result the_out_ref_form_is_unchanged(rclcpp::Node& node) {
    rclcpp::Publisher<CounterMsg> pub;
    return node.create_publisher(pub, "chatter");
}

} // namespace nros_cpp_ported_publisher_probe
