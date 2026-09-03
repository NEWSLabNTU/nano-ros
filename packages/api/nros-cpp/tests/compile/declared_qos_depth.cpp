// phase-403 step 2 -- the DECLARED `@depth=` reaches the compiler, and the
// three cases it has to tell apart all COMPILE.
//
// This is the POSITIVE half. `declared_qos_depth_probe.cpp` beside it is the
// negative half, and the negative half is the one that matters: a check that
// has never failed is not a check. Neither is worth anything alone -- an
// expected-failure compile cannot distinguish "the static_assert fired" from
// "the file is not there", which is why `just check cpp` asserts this TU
// compiles clean BEFORE it asserts the probe does not.
//
// The declared table comes from `declared-qos-fixture/nros/`, which is what
// `nros ws entity-inventory --output-header` renders from the `entities.json`
// beside it. In a real build `nano_ros_node_register()` writes that header into
// the component library's own include dir; here the gate puts the fixture dir
// on the include path, which exercises the same `__has_include` pickup.
//
// `just check cpp` compiles this with `-fsyntax-only -std=c++17`.
#include <nros/component_node.hpp>

namespace nros_cpp_declared_qos_compile_test {

// Generated-message shape. `TYPE_NAME` is the DDS-mangled spelling
// `packs/cpp/message.hpp.jinja` emits, which is the key the table carries.
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

// A type the fixture declares NO depth for. Its absence must read as "nobody
// said" and not as depth 0 -- so this one asserts nothing and keeps its
// historical default profile.
struct Bool {
    bool data{false};
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Bool_";
    static constexpr const char* TYPE_HASH = "RIHS01_bool_stub";
    static const size_t SERIALIZED_SIZE_MAX = 1;
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
};

// -- The LOOKUP itself, before any macro is involved ------------------------
//
// If these three drift, every assertion below becomes vacuous while still
// compiling, which is the failure mode this whole step was written against.

static_assert(::nros::declared_depth(Int32::TYPE_NAME, "/chatter") == 1,
              "the fixture declares sub:std_msgs/msg/Int32:/chatter@depth=1");

static_assert(::nros::declared_depth("std_msgs/msg/Int32", "/chatter") == 1,
              "the ROS spelling of the type is a key too -- a hand-written message class may "
              "carry it instead of the DDS-mangled one");

static_assert(::nros::declared_depth(Bool::TYPE_NAME, "/undeclared") ==
                  ::nros::DECLARED_DEPTH_UNDECLARED,
              "an endpoint that declared no depth is UNDECLARED, which is -1 and not 0. A 0 "
              "here would make every QoS(0)-free call site look like a mismatch, and would let "
              "a size consumer read a queue of zero as an answer");

static_assert(::nros::DECLARED_DEPTH_UNDECLARED != 0,
              "absence must never be spelled the same as a depth");

static_assert(::nros::declared_depth(Int32::TYPE_NAME, "/a_topic_nobody_declared") ==
                  ::nros::DECLARED_DEPTH_UNDECLARED,
              "the table is keyed on the PAIR: a declared type on an undeclared topic is not a "
              "hit, or one @depth= would size every topic that type is carried on");

// -- The QoS the 3-argument form fills in -----------------------------------

static_assert(::nros::detail::qos_from_declared_depth(::nros::declared_depth(Int32::TYPE_NAME,
                                                                             "/chatter"))
                      .depth() == 1,
              "mode 2: the contract states the depth and NROS_SUBSCRIBE(M, m, topic) takes it");

static_assert(::nros::detail::qos_from_declared_depth(::nros::declared_depth(Bool::TYPE_NAME,
                                                                             "/undeclared"))
                      .depth() == ::nros::QoS::default_profile().depth(),
              "and an undeclared endpoint keeps the historical default profile, so every call "
              "site that predates this step compiles and behaves unchanged");

// -- The macro, in the shape a component ctor actually writes ---------------

class Listener : public ::nros::ComponentNode {
  public:
    explicit Listener(::nros::NodeHandle h) : ::nros::ComponentNode(h, "listener") {
        // Mode 1: the code states the QoS and the declaration agrees.
        NROS_SUBSCRIBE(Int32, on_int, "/chatter", ::nros::QoS(1));
        // Mode 2: the code states no QoS, and the declared depth fills in.
        NROS_SUBSCRIBE(Int32, on_int, "/chatter");
        // Neither declared nor passed: not an error, not a default anybody
        // guessed -- the pre-existing QoS::default_profile().
        NROS_SUBSCRIBE(Bool, on_bool, "/undeclared");
        // Declared nowhere, QoS passed: nothing to disagree with.
        NROS_SUBSCRIBE(Bool, on_bool, "/undeclared", ::nros::QoS(3));
        // A constant expression that is not a literal is still a constant
        // expression, so mode 1 works through one.
        NROS_SUBSCRIBE(Int32, on_int, kChatter, ::nros::QoS(1));
        // The escape hatch for a topic that is NOT a constant expression. It
        // takes the boot-time check instead; see create_subscription.
        NROS_SUBSCRIBE_DYNAMIC(Int32, on_int, runtime_topic(), ::nros::QoS(1));
    }

  private:
    static constexpr const char* kChatter = "/chatter";
    const char* runtime_topic() const { return topic_; }
    const char* topic_ = "/chatter";
    void on_int(const Int32&) {}
    void on_bool(const Bool&) {}
};

} // namespace nros_cpp_declared_qos_compile_test
