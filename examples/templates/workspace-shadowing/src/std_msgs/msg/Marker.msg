# Phase 210.F.4 shadowing-fixture marker.
#
# This message is the SHADOWING SIGNAL — upstream ROS 2 std_msgs ships
# no `Marker.msg`. If the consumer's `find_package(std_msgs)` resolved
# to the AMENT-installed std_msgs instead of THIS workspace copy, the
# `std_msgs/msg/marker.hpp` include would fail at compile time AND the
# `.shadowed_marker` field reference would be undeclared.
#
# The field name `shadowed_marker` is the nm-grep signal: the consumer
# binary's symbol table contains `std_msgs::msg::Marker_*` accessors
# referencing this unique field name.
string shadowed_marker
