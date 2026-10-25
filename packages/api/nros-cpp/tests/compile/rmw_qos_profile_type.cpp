// issue 2054 — upstream's `rmw_qos_profile_t`.
//
// Humble-era rclcpp code names the rmw struct directly: it copies a named
// profile, edits a field, builds a `QoS` from it and reads one back. Each form
// below is one the issue lists as failing to compile before the type existed.
// Compiled `-fsyntax-only` by `just check cpp`; every claim about a VALUE is a
// `static_assert`, so the check needs no runtime.

#include <rclcpp/rclcpp.hpp>

#include <type_traits>

namespace {

// -- the type and its upstream numbering ------------------------------------

static_assert(std::is_same<rmw_qos_profile_t, rmw_qos_profile_s>::value,
              "rmw_qos_profile_t is upstream's typedef of rmw_qos_profile_s");
static_assert(std::is_same<decltype(rmw_qos_profile_t::depth), size_t>::value,
              "depth is size_t, as upstream's is");
static_assert(std::is_same<decltype(rmw_qos_profile_t::deadline), rmw_time_t>::value,
              "the windows are rmw_time_t, as upstream's are");
// Upstream's enumerator values (rmw/types.h). These are NOT `rclcpp::`'s, whose
// `Reliable` is 0; a cast between the two vocabularies would be wrong.
static_assert(RMW_QOS_POLICY_RELIABILITY_RELIABLE == 1, "upstream numbering");
static_assert(RMW_QOS_POLICY_RELIABILITY_BEST_EFFORT == 2, "upstream numbering");
static_assert(RMW_QOS_POLICY_HISTORY_KEEP_LAST == 1, "upstream numbering");
static_assert(RMW_QOS_POLICY_HISTORY_KEEP_ALL == 2, "upstream numbering");
static_assert(RMW_QOS_POLICY_DURABILITY_TRANSIENT_LOCAL == 1, "upstream numbering");
static_assert(RMW_QOS_POLICY_DURABILITY_VOLATILE == 2, "upstream numbering");
static_assert(RMW_QOS_POLICY_LIVELINESS_MANUAL_BY_NODE == 2, "upstream numbering");
static_assert(RMW_QOS_POLICY_LIVELINESS_MANUAL_BY_TOPIC == 3, "upstream numbering");

// -- the five constants are upstream's values (rmw/qos_profiles.h) ----------

static_assert(rmw_qos_profile_default.history == RMW_QOS_POLICY_HISTORY_KEEP_LAST &&
                  rmw_qos_profile_default.depth == 10 &&
                  rmw_qos_profile_default.reliability == RMW_QOS_POLICY_RELIABILITY_RELIABLE &&
                  rmw_qos_profile_default.durability == RMW_QOS_POLICY_DURABILITY_VOLATILE &&
                  rmw_qos_profile_default.liveliness == RMW_QOS_POLICY_LIVELINESS_SYSTEM_DEFAULT &&
                  rmw_qos_profile_default.deadline.sec == 0 &&
                  rmw_qos_profile_default.deadline.nsec == 0 &&
                  !rmw_qos_profile_default.avoid_ros_namespace_conventions,
              "rmw_qos_profile_default");
static_assert(rmw_qos_profile_sensor_data.depth == 5 &&
                  rmw_qos_profile_sensor_data.reliability == RMW_QOS_POLICY_RELIABILITY_BEST_EFFORT,
              "rmw_qos_profile_sensor_data");
static_assert(rmw_qos_profile_services_default.depth == 10 &&
                  rmw_qos_profile_services_default.reliability ==
                      RMW_QOS_POLICY_RELIABILITY_RELIABLE,
              "rmw_qos_profile_services_default");
static_assert(rmw_qos_profile_parameters.depth == 1000, "rmw_qos_profile_parameters");
static_assert(rmw_qos_profile_parameter_events.depth == 1000, "rmw_qos_profile_parameter_events");

// ... and still convert to `rclcpp::QoS`, to the table's row.
static_assert(static_cast<rclcpp::QoS>(rmw_qos_profile_sensor_data) == rclcpp::SensorDataQoS(),
              "a constant converts to the QoS it was lowered from");
static_assert(static_cast<rclcpp::QoS>(rmw_qos_profile_services_default) == rclcpp::ServicesQoS(),
              "a constant converts to the QoS it was lowered from");

// -- windows: upstream's sentinels map onto ours and back -------------------

constexpr rmw_qos_profile_t with_windows() {
    rmw_qos_profile_t p = rmw_qos_profile_default;
    p.deadline = rmw_time_t{1, 500000000};          // 1.5 s
    p.lifespan = RMW_DURATION_INFINITE;             // explicit infinite
    p.liveliness_lease_duration = rmw_time_t{0, 1}; // 1 ns rounds UP to 1 ms
    return p;
}
static_assert(static_cast<rclcpp::QoS>(with_windows()).deadline().nanoseconds() == 1500000000,
              "a whole-millisecond window converts exactly");
static_assert(static_cast<rclcpp::QoS>(with_windows()).liveliness_lease_duration().nanoseconds() ==
                  1000000,
              "a sub-millisecond window rounds up, never down to 'no window'");
static_assert(static_cast<rclcpp::QoS>(with_windows()).get_rmw_qos_profile().lifespan.nsec ==
                  854775807u,
              "RMW_DURATION_INFINITE round-trips");

// -- building a QoS from a profile keeps the profile's policies -------------

constexpr rmw_qos_profile_t edited() {
    rmw_qos_profile_t p = rmw_qos_profile_default;
    p.depth = 1;
    p.reliability = RMW_QOS_POLICY_RELIABILITY_BEST_EFFORT;
    p.durability = RMW_QOS_POLICY_DURABILITY_TRANSIENT_LOCAL;
    return p;
}
static_assert(rclcpp::QoS(rclcpp::QoSInitialization::from_rmw(edited()), edited()) ==
                  rclcpp::QoS(1).best_effort().transient_local(),
              "QoS(from_rmw(p), p) is p");
static_assert(rclcpp::QoS(rclcpp::KeepLast(3), rmw_qos_profile_sensor_data) ==
                  rclcpp::SensorDataQoS().keep_last(3),
              "QoS(KeepLast(n), p) is p at depth n");
static_assert(rclcpp::QoS(rclcpp::KeepAll(), edited()).history() == rclcpp::HistoryPolicy::KeepAll,
              "the initialisation's history wins over the profile's");

// -- the four forms issue 2054 lists ----------------------------------------

[[maybe_unused]] void four_forms() {
    // 1. Name the type and copy a named profile into it.
    rmw_qos_profile_t profile = rmw_qos_profile_default;
    // 2. Read and write its fields.
    profile.depth = 1;
    profile.history = RMW_QOS_POLICY_HISTORY_KEEP_LAST;
    profile.reliability = RMW_QOS_POLICY_RELIABILITY_BEST_EFFORT;
    profile.durability = RMW_QOS_POLICY_DURABILITY_TRANSIENT_LOCAL;
    profile.deadline = RMW_QOS_DEADLINE_DEFAULT;
    // 3. Build a QoS from it, upstream's way.
    rclcpp::QoS qos(rclcpp::QoSInitialization::from_rmw(profile), profile);
    rclcpp::QoS keep_last_qos(rclcpp::KeepLast(3), rmw_qos_profile_sensor_data);
    // 4. Read the struct back.
    rmw_qos_profile_t back = qos.get_rmw_qos_profile();
    (void)back.depth;
    (void)keep_last_qos;
}

} // namespace
