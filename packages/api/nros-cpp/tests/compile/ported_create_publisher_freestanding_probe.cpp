// EXPECTED-FAILURE probe — phase-427 W3.
//
// A ported `node->create_publisher<M>("chatter", 10)` must FAIL TO COMPILE on a
// freestanding target, and the diagnostic must name the out-ref overload that
// IS available there.
//
// This is the governing principle's whole mechanism, at its sharpest point. The
// hosted overload returns `std::shared_ptr<Publisher<M>>` and there is no
// allocator on this target, so the honest answers are "fails to compile" or
// "compiles and allocates". The second is the one RFC-0089 forbids: a firmware
// image that silently gained a heap allocation per publisher would be a
// contract change nobody was told about.
//
// It fails for a STRUCTURAL reason rather than a `static_assert`: the hosted
// signatures are gated on `NROS_CPP_NODE_HOSTED`, which is off here, so the
// overload does not exist and overload resolution reports the ones that do.
// That is why the check below greps the diagnostic — "it failed" is also what a
// typo produces, and this probe must distinguish the two.
//
// Compiled `-nostdinc++` against the ThreadX minimal libcpp. Its positive twin
// is `one_node_type_freestanding.cpp`.

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

inline void a_ported_line_must_not_compile_here() {
    rclcpp::Node node("talker");
    // THE LINE. Hosted, this returns a `std::shared_ptr<Publisher<CounterMsg>>`.
    // Here there is no such overload, and the migration is the out-ref form:
    //     rclcpp::Publisher<CounterMsg> pub;
    //     node.create_publisher(pub, "chatter");
    auto pub = node.create_publisher<CounterMsg>("chatter", 10);
    (void)pub;
}

} // namespace nros_cpp_ported_publisher_probe
