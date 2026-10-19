// issue 1975 — four accessors Jazzy's rclcpp added, each a read over state the
// node, publisher, timer or logger already holds:
//
//   * `rclcpp::Logger::get_effective_level()`
//   * `rclcpp::PublisherBase::is_durability_transient_local()`
//   * `rclcpp::TimerBase::get_clock()`
//   * `rclcpp::NodeOptions::clock_type()` (getter + builder setter)
//
// A RUN rather than a `static_assert`, because every one of them would COMPILE
// as an inert stub: a `clock_type` setter nothing reads, a `get_clock` that
// answers the node's clock whatever the timer runs on, a transient-local test
// over the requested rather than the granted QoS. The question is what the
// created entity ANSWERS, which only a created entity can say.
//
// The RMW backend is the shared stub (`nros-c/tests/run/stub_rmw_backend.c`):
// a node needs an executor, an executor needs a session, nothing here touches
// the wire.

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

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

void on_tick(void*) {}

} // namespace

int main() {
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i1975").ok()) {
        std::fprintf(stderr, "jazzy_small_accessors_runtime: init on the stub backend failed\n");
        return 1;
    }

    // --- NodeOptions::clock_type ---------------------------------------------
    {
        rclcpp::NodeOptions defaults;
        check(defaults.clock_type() == RCL_ROS_TIME,
              "NodeOptions{}.clock_type() must default to RCL_ROS_TIME, upstream's default and "
              "what a node clock was before the option existed");

        rclcpp::NodeOptions& chained = defaults.clock_type(RCL_STEADY_TIME);
        check(&chained == &defaults, "clock_type(t) must return *this so it chains");
        check(defaults.clock_type() == RCL_STEADY_TIME, "clock_type(t) must store t");
    }

    rclcpp::Node plain("i1975_plain");
    check(plain.ok(), "a node without options must come up");
    check(plain.get_clock()->get_clock_type() == RCL_ROS_TIME,
          "a node constructed without options keeps its ROS-time clock");
    check(plain.get_node_options().clock_type() == RCL_ROS_TIME,
          "get_node_options() on a node given no options must answer the default");

    rclcpp::Node steady("i1975_steady", rclcpp::NodeOptions().clock_type(RCL_STEADY_TIME));
    check(steady.ok(), "a node with clock_type(RCL_STEADY_TIME) must come up");
    check(steady.get_clock()->get_clock_type() == RCL_STEADY_TIME,
          "NodeOptions::clock_type must reach the node clock -- an option the constructor "
          "never reads is the inert-setter inversion RFC-0089 W3.f removed");
    check(steady.get_node_options().clock_type() == RCL_STEADY_TIME,
          "get_node_options() must hand back the options the node was given, not a constant");

    rclcpp::Node bogus("i1975_bogus", rclcpp::NodeOptions().clock_type(RCL_CLOCK_UNINITIALIZED));
    check(!bogus.ok(),
          "a clock_type that is not a clock must latch ok() == false, where upstream throws");
    check(bogus.get_clock()->is_valid(),
          "...and must leave the node clock usable rather than reading zero forever");

    // --- Timer::get_clock -----------------------------------------------------
    // The handle path (`Timer::SharedPtr`, what ported code holds) answers
    // from the executor too.
    rclcpp::TimerBase::SharedPtr wall_handle =
        plain.create_wall_timer(rclcpp::Duration::from_nanoseconds(100000000), [] {});
    check(static_cast<bool>(wall_handle), "create a wall timer through the ported verb");
    check(wall_handle->get_clock()->get_clock_type() == RCL_STEADY_TIME,
          "a ported wall timer's get_clock() must answer a steady clock");
    rclcpp::TimerBase::SharedPtr ros_handle = rclcpp::create_timer(
        plain, plain.get_clock(), rclcpp::Duration::from_nanoseconds(100000000), [] {});
    check(static_cast<bool>(ros_handle), "create a ROS-clock timer through the ported verb");
    check(ros_handle->get_clock()->get_clock_type() == RCL_ROS_TIME,
          "a ported create_timer(node, clock, ...) handle must answer that clock");
    rclcpp::TimerBase::SharedPtr stale_copy = ros_handle;
    ros_handle.reset();
    check(!ros_handle->get_clock(), "a released handle's get_clock() must test false");
    check(!stale_copy->get_clock(),
          "a STALE copy of a released handle must test false: the executor no longer knows "
          "the timer, so there is no clock to answer");
    // Released so the C-callback timers below fit the default executor's
    // timer slots.
    wall_handle.reset();

    rclcpp::Timer unset;
    check(!unset.get_clock(), "an uninitialized timer's get_clock() must test false");
    check(!unset.get_clock()->is_valid(), "...and hold an invalid clock");

    rclcpp::Timer wall;
    check(plain.create_wall_timer(wall, 100, on_tick).ok(), "create a wall timer");
    check(static_cast<bool>(wall.get_clock()), "a registered timer's get_clock() tests true");
    check(wall.get_clock()->get_clock_type() == RCL_STEADY_TIME,
          "a wall timer runs on a steady clock, as rclcpp's do");

    rclcpp::Timer ros;
    check(plain.create_timer(ros, *plain.get_clock(), 100, on_tick).ok(),
          "create a timer on the node's ROS clock");
    check(ros.get_clock()->get_clock_type() == RCL_ROS_TIME,
          "a create_timer(clock, ...) timer must answer the clock it was created with, not a "
          "steady default");

    rclcpp::Timer system;
    rclcpp::Clock system_clock(RCL_SYSTEM_TIME);
    check(plain.create_timer(system, system_clock, 100, on_tick).ok(),
          "create a timer on a system clock");
    check((*system.get_clock()).get_clock_type() == RCL_SYSTEM_TIME,
          "operator* reaches the same clock as operator->");

    rclcpp::TimerHandle empty;
    check(!empty->get_clock(), "an empty handle's get_clock() must test false");

    // --- Publisher::is_durability_transient_local -----------------------------
    rclcpp::Publisher<CounterMsg> unset_pub;
    check(!unset_pub.is_durability_transient_local(),
          "an uninitialized publisher is not transient-local");

    // The stub refuses entity creation unless told to accept it.
    nros_stub_rmw_set_accept_entities(true);
    rclcpp::Publisher<CounterMsg> latched;
    check(plain
              .create_publisher<CounterMsg>(latched, "/i1975/latched",
                                            rclcpp::QoS(1).transient_local())
              .ok(),
          "create a transient-local publisher");
    rclcpp::Publisher<CounterMsg> volatile_pub;
    check(plain
              .create_publisher<CounterMsg>(volatile_pub, "/i1975/volatile",
                                            rclcpp::QoS(1).durability_volatile())
              .ok(),
          "create a volatile publisher");

    // The answer is the GRANTED durability's, as upstream's reads
    // `rcl_publisher_get_actual_qos`. The stub grants what was requested
    // (its `publisher_get_actual_qos` echoes the create profile), so the
    // granted answer is known here.
    check(latched.get_actual_qos().durability() == rclcpp::DurabilityPolicy::TransientLocal,
          "precondition: the stub must report the transient-local grant");
    check(latched.is_durability_transient_local(),
          "a publisher granted TRANSIENT_LOCAL must read true");
    check(!volatile_pub.is_durability_transient_local(),
          "a publisher granted VOLATILE must read false");

    // --- Logger::get_effective_level ------------------------------------------
    rclcpp::Logger log = plain.get_logger();
    check(log.set_level(rclcpp::Logger::Level::Warn).ok(), "set the node logger to WARN");
    check(log.get_effective_level() == rclcpp::Logger::Level::Warn,
          "get_effective_level must read back the logger's own level");
    check(log.set_level(rclcpp::Logger::Level::Unset).ok(), "take the logger's own level away");
    check(log.get_effective_level() != rclcpp::Logger::Level::Unset,
          "the EFFECTIVE level is never Unset: it falls back to the process default");
    check(log.get_effective_level() == log.get_level(),
          "get_effective_level and get_level must be one answer");
    rclcpp::Logger nameless("i1975_free");
    check(nameless.get_effective_level() == rclcpp::Logger::Level::Fatal,
          "a handle-less logger answers Fatal, the quiet end");

    if (g_failures != 0) {
        std::fprintf(stderr, "jazzy_small_accessors_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("jazzy_small_accessors_runtime: ok\n");
    return 0;
}
