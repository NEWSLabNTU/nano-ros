// phase-427 W1-W3 / W5, phase-430 W6 — POSITIVE probe: there is ONE node type,
// it carries BOTH creation shapes, and every ported spelling still binds.
//
// Compiled HOSTED, because the half this file is about that a freestanding
// target does not get is the `shared_ptr` family. The freestanding half has its
// own probe (`one_node_type_freestanding.cpp`), compiled `-nostdinc++` against
// the ThreadX minimal libcpp, and the NEGATIVE probe
// (`ported_create_publisher_freestanding_probe.cpp`) proves that a ported
// `create_publisher<M>("chatter", 10)` FAILS THERE rather than differing.
//
// The lane compiles this one with `-DNROS_CPP_STD=1`, and it has to since
// phase-438 W2: the hosted shape is a REQUEST now, not a property of the
// toolchain. Everything below `NROS_CPP_NODE_HOSTED` — the `shared_ptr` and
// `std::string` families AND the whole parameter facade, `const char*`-keyed
// forms included — is absent without it. A `rclcpp::Node` still EXISTS with no
// flag, which is phase-427's point and what
// `rclcpp_node_freestanding_surface.cpp` instantiates; it is a smaller surface,
// which is phase-438's.
//
// WHAT THIS PROVES
//   1. `rclcpp::Node` and `nros::Node` are the SAME TYPE, not two types with a
//      converting constructor between them. This is the whole item: before the
//      merge a ported file got a type with no graph queries, no lifecycle, no
//      callback groups and no out-ref creators, while a native file got a type
//      with no `shared_ptr` creators and no parameters.
//   2. Upstream's constructor compiles VERBATIM, and so does the freestanding
//      `Node("talker")` + `init()` + `ok()` shape, on the same type.
//   3. ONE NAME, TWO SIGNATURES: the out-ref and `shared_ptr` `create_*`
//      families are overloads on that type and neither is ambiguous.
//   4. `create_wall_timer` binds a MEMBER FUNCTION with no allocation and no
//      `std::function` — the overload that retired `nros::bind_timer`.
//   5. `rclcpp::create_timer(node, clock, period, cb)` — humble's only
//      clock-taking timer verb (phase-430 W6) — compiles and returns the same
//      cell `create_wall_timer` does.
//   6. `get_logger()` is NAMED FOR THE NODE (phase-427 W5) and still converts
//      to the opaque `nros_logger_t` the `NROS_LOG_*` macros take.
//
// WHAT IT DOES NOT PROVE
//   That any callback FIRES. That needs a session, a backend and a peer; the
//   runtime cells live in `nros_tests`. The structural half is what a compile
//   lane can hold, and it is the half that regresses silently.

#include <nros/nros.hpp>

#include <chrono>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>
#include <type_traits>

namespace nros_cpp_one_node_type_test {

struct CounterMsg {
    int32_t data = 0;
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

// --- (1) ONE TYPE ------------------------------------------------------------
//
// `std::is_same`, not `is_convertible` or `is_base_of`: an alias is the claim,
// and any wrapper — even an implicitly-converting one — would reintroduce two
// objects, two entity sets and the two-parameter-store duplication `nros.hpp`
// used to flag against itself.

static_assert(std::is_same<::rclcpp::Node, ::nros::Node>::value,
              "rclcpp::Node and nros::Node have come apart again -- phase-427 merged them, and "
              "two node types is what put a second parameter facade, a second get_logger() and "
              "two disjoint create_* families in one package");

// The deleted shim was `std::enable_shared_from_this<Node>`, a hosted-only BASE
// carrying a weak_ptr member -- 16 bytes of layout behind a capability probe,
// which is exactly what `check-cpp-capability-layout` forbids. The verb
// survives as a method; the base must not come back.
static_assert(!std::is_base_of<std::enable_shared_from_this<::rclcpp::Node>, ::rclcpp::Node>::value,
              "rclcpp::Node derives from enable_shared_from_this again -- that base's weak_ptr "
              "member exists only where <memory> does, so sizeof(Node) would follow a probe");

// --- (2) both constructions, on the one type --------------------------------

/// Upstream's own text. Nothing here is a nano-ros spelling.
inline void upstream_construction_verbatim() {
    auto node = std::make_shared<rclcpp::Node>("talker");
    auto with_ns = std::make_shared<rclcpp::Node>("talker", "/demo");
    auto with_options = std::make_shared<rclcpp::Node>("talker", rclcpp::NodeOptions());
    (void)node->get_name();
    (void)with_ns->get_namespace();
    (void)with_options->get_node_options();
    // `shared_from_this()` is a METHOD here, not a base class. See its doc for
    // the ownership weakening that buys.
    (void)node->shared_from_this();
}

/// The freestanding shape, written on the same type. `init()` is the channel a
/// `-fno-exceptions` target needs in place of a throwing constructor, and
/// `ok()` is what replaces the throw.
inline void freestanding_construction() {
    rclcpp::Node node("talker");
    if (!node.ok()) return;

    rclcpp::Node deferred;
    if (!deferred.init("listener", "/demo").ok()) return;
    (void)deferred.ok();
}

// --- (3) one name, two signatures -------------------------------------------

inline void both_create_families_on_one_object() {
    rclcpp::Node node("talker");

    // Out-ref: caller-owned storage, no allocation. Forced by the arena, which
    // stores `&entity` as its dispatch context and has no unregister.
    nros::Publisher<CounterMsg> pub;
    (void)node.create_publisher(pub, "chatter");
    nros::Subscription<CounterMsg> sub;
    (void)node.create_subscription(
        sub, "chatter", +[](const CounterMsg&) {});

    // shared_ptr: upstream's signatures, on the same object.
    auto pub2 = node.create_publisher<CounterMsg>("chatter", 10);
    auto sub2 = node.create_subscription<CounterMsg>("chatter", 10, [](const CounterMsg&) {});
    auto pub3 = node.create_publisher<CounterMsg>("chatter", rclcpp::QoS(10));
    (void)pub2;
    (void)sub2;
    (void)pub3;
}

// --- (4) member binding with no allocation ----------------------------------
//
// This is the component shape RFC-0089 recommends for firmware: no allocator,
// no derivation, no vtable. The member pointer is a TEMPLATE PARAMETER, so the
// trampoline is a capture-less lambda that converts to the executor's raw
// `void(*)(void*)` -- there is no `std::function` and no heap cell anywhere in
// the path, which is why this overload reaches every target while the
// `std::chrono` one does not.

class Talker {
    nros::Publisher<CounterMsg> pub_;
    nros::Timer timer_;
    int count_ = 0;

  public:
    void on_tick() { ++count_; }

    nros::Result configure(rclcpp::Node& node) {
        NROS_TRY(node.create_publisher(pub_, "chatter"));
        return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
    }
};

/// The binding overload must not need `<functional>`: `std::function` in the
/// path is what makes the hosted `create_wall_timer(duration, callable)`
/// hosted-only, and the whole point of this one is that a component reaches it
/// on a target with no allocator.
static_assert(
    std::is_same<decltype(std::declval<rclcpp::Node&>().create_wall_timer<Talker, &Talker::on_tick>(
                     std::declval<nros::Timer&>(), uint64_t(0), std::declval<Talker*>())),
                 nros::Result>::value,
    "the member-binding create_wall_timer overload has changed shape -- it is what "
    "retired the free nros::bind_timer, and a component binds through it");

// --- (5) phase-430 W6: rclcpp::create_timer(node, clock, period, cb) --------

inline void clock_driven_timer_is_humbles_free_verb() {
    auto node = std::make_shared<rclcpp::Node>("bagged");

    // Humble has no `Node::create_timer` member; the clock-taking verb is free.
    // `node->get_clock()` hands back exactly the parameter type, so the ported
    // line binds with no conversion.
    auto ros_time =
        rclcpp::create_timer(node, node->get_clock(), std::chrono::milliseconds(100), []() {});

    // The steady verb in the same file is unaffected by /clock.
    auto wall = node->create_wall_timer(std::chrono::milliseconds(100), []() {});

    // Both hand back the same cell type -- there is one flat `Timer`, and
    // phase-430 W7 deleted the `TimerBase` the two used to be typed as.
    static_assert(std::is_same<decltype(ros_time), decltype(wall)>::value,
                  "create_timer and create_wall_timer must return the same handle type");
    static_assert(std::is_same<decltype(wall), std::shared_ptr<::nros::Timer>>::value,
                  "the timer handle must be rclcpp::Timer::SharedPtr");

    // The nros::Duration spelling of the same call.
    auto by_duration =
        rclcpp::create_timer(node, node->get_clock(), nros::Duration::from_seconds(0.1), []() {});
    (void)ros_time;
    (void)wall;
    (void)by_duration;
}

// --- (6) phase-427 W5: the logger is named for the node ---------------------
//
// The RETURN TYPE is upstream's, because two `get_logger()` overloads differing
// only in return type are ill-formed and the ported channel wins (RFC-0089
// clause 2). The native call sites keep working through the implicit conversion
// to `nros_logger_t`, which is what this pins.

static_assert(
    std::is_same<decltype(std::declval<const rclcpp::Node&>().get_logger()), rclcpp::Logger>::value,
    "Node::get_logger() must return rclcpp::Logger -- upstream's channel");

static_assert(std::is_convertible<rclcpp::Logger, const void*>::value,
              "rclcpp::Logger must still convert to nros_logger_t, or every NROS_LOG_* call "
              "site that takes node.get_logger() stops compiling");

inline void two_nodes_two_logger_names() {
    rclcpp::Node a("talker");
    rclcpp::Node b("listener");
    // NAMED FOR THE NODE, not the "nros.compat" sentinel the shim returned.
    // A runtime cell asserts the emitted records differ; what a compile lane
    // can hold is that the name comes from the node at all.
    const char* an = a.get_logger().get_name();
    const char* bn = b.get_logger().get_name();
    (void)an;
    (void)bn;
    // The opaque handle still reaches the log macros.
    NROS_LOG_INFO(a.get_logger(), "up");
}

// --- parameters, on the one facade ------------------------------------------

inline void parameters_are_one_facade() {
    rclcpp::Node node("talker");
    const double period = node.declare_parameter<double>("period", 0.15);
    const bool found = node.get_parameter<double>("period", const_cast<double&>(period));
    (void)node.has_parameter("period");
    (void)node.declare_parameter<int64_t>(std::string("count"), 3);
    (void)found;
}

inline void probe_entry_points() {
    upstream_construction_verbatim();
    freestanding_construction();
    both_create_families_on_one_object();
    clock_driven_timer_is_humbles_free_verb();
    two_nodes_two_logger_names();
    parameters_are_one_facade();
    Talker t;
    rclcpp::Node n("talker");
    (void)t.configure(n);
}

} // namespace nros_cpp_one_node_type_test
