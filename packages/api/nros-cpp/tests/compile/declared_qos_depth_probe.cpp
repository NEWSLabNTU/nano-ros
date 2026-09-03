// phase-403 step 2 -- the NEGATIVE half: this TU MUST NOT COMPILE.
//
// The declaration in `declared-qos-fixture/entities.json` says
// `sub:std_msgs/msg/Int32:/chatter@depth=1`. The call site below passes
// `nros::QoS(10)`. One subscription, two different depths, and depth is a
// multiplier on the executor arena -- so the build has to stop.
//
// WHY THIS FILE EXISTS AT ALL. This campaign has produced six sizing mechanisms
// that were correct and unreachable: the code was right, and nothing ever ran
// it. A compile-time check is the easiest of all of them to leave vacuous,
// because a table that never matches, a macro arm never selected and a topic
// spelled differently in the declaration all look exactly like "no mismatch
// here". The only evidence that the check is REACHABLE is a case where it
// fires, so here is one, and `just check cpp` compiles it expecting failure
// AND greps the diagnostic for the topic and both numbers -- a rejection for
// the wrong reason (a typo, a missing include) would otherwise read as a pass.
//
// The positive TU beside this one is asserted to compile clean FIRST, for the
// same reason `qos_deprecation_probe.cpp` is: an expected-failure compile
// cannot tell "the assertion fired" from "the file is not there".
#include <nros/component_node.hpp>

namespace nros_cpp_declared_qos_probe {

struct Int32 {
    int32_t data{0};
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static const size_t SERIALIZED_SIZE_MAX = 8;
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

class Listener : public ::nros::ComponentNode {
  public:
    explicit Listener(::nros::NodeHandle h) : ::nros::ComponentNode(h, "listener") {
        // DECLARED @depth=1. PASSED depth 10. This line is the whole test.
        NROS_SUBSCRIBE(Int32, on_int, "/chatter", ::nros::QoS(10));
    }

  private:
    void on_int(const Int32&) {}
};

} // namespace nros_cpp_declared_qos_probe
