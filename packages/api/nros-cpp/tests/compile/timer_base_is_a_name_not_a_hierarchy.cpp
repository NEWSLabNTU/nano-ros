// phase-430 W7 — POSITIVE probe: `rclcpp::TimerBase` is a NAME, not a hierarchy.
//
// The ruling deleted the HIERARCHY and KEPT the NAME, and the two halves need
// pinning in opposite directions, because each has its own way of coming back.
//
// WHY THE NAME IS KEPT, measured rather than argued. `just colcon-parity` builds
// `examples/templates/local-msg-package` against a REAL `/opt/ros/<distro>`
// install, and `examples/templates/cpp-port-minimal-publisher` is vendored
// UNMODIFIED to demonstrate that upstream source compiles here. Both declare a
// timer member. Deleting `rclcpp::TimerBase` outright failed that gate with
//
//     'Timer' in namespace 'rclcpp' does not name a type; did you mean 'Time'?
//
// from REAL rclcpp — upstream has no `rclcpp::Timer`. So without this alias the
// intersection of "compiles under real ROS 2" and "compiles under nano-ros" is
// EMPTY for any ported node holding a timer, which is most of them. The alias is
// load-bearing for the campaign's own flagship property, not a courtesy, and it
// is therefore NOT deprecated: deprecating it would promise a removal that would
// break that property on purpose.
//
// WHAT MUST NOT COME BACK is the hierarchy. `ros2_one_dispatch_path.cpp` holds
// that end — `!is_polymorphic<detail::WallTimer>` plus `create_wall_timer`'s
// return type. This file holds the other: the two spellings are ONE FLAT TYPE,
// so the name cannot quietly regrow a base underneath it.
//
// If someone reintroduces `class TimerBase { virtual ~TimerBase(); }` with
// `Timer` under it, `is_same` here fails immediately, and the diagnostic points
// at the paragraph above rather than at a link error three lanes later.

#include <nros/nros.hpp>

#include <memory>
#include <type_traits>

namespace nros_cpp_timer_base_test {

// ONE TYPE, TWO SPELLINGS. Not a base, not a wrapper, not a converting alias.
static_assert(std::is_same<rclcpp::TimerBase, rclcpp::Timer>::value,
              "rclcpp::TimerBase must be an ALIAS for the flat rclcpp::Timer. If it has become a "
              "class again, phase-430 W7's ruling has been reverted: the executor dispatches "
              "through a raw function pointer, so a polymorphic base is a vtable no dispatch "
              "uses");

static_assert(std::is_same<rclcpp::TimerBase, ::nros::Timer>::value,
              "the ported name must resolve to nros::Timer -- one timer type across both "
              "vocabularies");

// No vtable behind either spelling. This is the cost half of the ruling: a
// polymorphic timer would put a vtable pointer in every image that holds one.
static_assert(!std::is_polymorphic<rclcpp::TimerBase>::value,
              "rclcpp::TimerBase has regained a vtable");

// The nested aliases a ported member declaration reaches for, under BOTH
// spellings, and they must name the same thing.
static_assert(std::is_same<rclcpp::TimerBase::SharedPtr, rclcpp::Timer::SharedPtr>::value,
              "TimerBase::SharedPtr and Timer::SharedPtr must be the same type");
static_assert(std::is_same<rclcpp::TimerBase::SharedPtr, std::shared_ptr<::nros::Timer>>::value,
              "TimerBase::SharedPtr must be std::shared_ptr<nros::Timer>");

/// The two member declarations the dual-compile templates actually contain.
/// Both must bind, and `create_wall_timer`'s return value must assign to both,
/// or those templates stop compiling under nano-ros while still compiling under
/// real ROS 2 — which is the half of the property a nano-ros-only lane cannot
/// see.
struct PortedMemberDeclarations {
    std::shared_ptr<rclcpp::TimerBase> upstream_spelling; // local-msg-package
    rclcpp::TimerBase::SharedPtr nested_spelling;         // cpp-port-minimal-publisher
};

#ifdef NROS_CPP_HAS_STD_CHRONO
inline void the_returned_handle_assigns_to_both_spellings(rclcpp::Node& node) {
    PortedMemberDeclarations m;
    m.upstream_spelling = node.create_wall_timer(std::chrono::milliseconds(100), []() {});
    m.nested_spelling = node.create_wall_timer(std::chrono::milliseconds(100), []() {});
    (void)m;
}
#endif

// WHAT THE NAME STILL DOES NOT PROMISE. `WallTimer` and `GenericTimer` are
// upstream's siblings under `TimerBase` and stay ABSENT: the clock axis is a
// runtime field plus the second verb `rclcpp::create_timer`, not a type
// parameter, so aliasing them would claim a genericity we cannot deliver. A
// ported file naming either fails to compile, which is the honest outcome; this
// detector is here so "we still do not have them" is asserted rather than
// assumed.
template <typename T, typename = void> struct has_wall_timer : std::false_type {};
template <typename T>
struct has_wall_timer<T, decltype(void(sizeof(typename T::WallTimer)))> : std::true_type {};

struct WithWallTimer {
    struct WallTimer {};
};
struct WithoutWallTimer {};

// Self-test of the detector, so the assertion below cannot be vacuous.
static_assert(has_wall_timer<WithWallTimer>::value, "the WallTimer detector does not detect one");
static_assert(!has_wall_timer<WithoutWallTimer>::value, "the WallTimer detector fires on absence");

} // namespace nros_cpp_timer_base_test
