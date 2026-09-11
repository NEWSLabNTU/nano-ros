// EXPECTED-FAILURE probe — phase-427 W7.
//
// `nros::Node` is DEPRECATED. `rclcpp::Node` is the class (RFC-0089 §"Settled:
// `rclcpp::` is the HOME") and `nros::` is the spelling that phases out; the
// alias survives so an out-of-tree consumer — `nros-v0.5.0` shipped the old
// spelling in six `examples/templates/**` files a user copies out — gets the
// migration in the compiler's own words rather than as an unknown identifier.
//
// A deprecation nobody is told about is just an alias, so this probe asserts the
// attribute FIRES: under `-Werror=deprecated-declarations` the old spelling must
// fail to compile, and the lane greps the diagnostic for the replacement,
// because "it failed" is also what a typo produces.
//
// Same shape as `bind_timer_deprecation_probe.cpp`, `qos_deprecation_probe.cpp`,
// `receive_deprecation_probe.cpp` and `expected_deprecation_probe.cpp`. The
// POSITIVE twin — both spellings still naming ONE type — is
// `one_node_type.cpp`, which is the one TU in the tree deliberately left
// un-migrated, because its subject IS the old spelling.

#include <nros/nros.hpp>

namespace nros_cpp_node_deprecation_probe {

// The deprecated spelling in the shape a ported file writes it: a parameter
// type. `[[deprecated]]` on an alias fires where the NAME is used, so this is
// enough and it needs no definition.
::rclcpp::Result the_retired_spelling(::nros::Node& node);

} // namespace nros_cpp_node_deprecation_probe
