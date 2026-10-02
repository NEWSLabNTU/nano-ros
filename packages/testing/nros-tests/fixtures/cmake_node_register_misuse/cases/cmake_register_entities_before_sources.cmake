# Case `cmake_register_entities_before_sources` — see ../CMakeLists.txt.
#
# Retired ENTITIES before any multi-value keyword — lands in
# UNPARSED_ARGUMENTS, which nothing reads. Must raise the tombstone.
set(VERDICT_PROJECT talker_pkg)
macro(verdict_body)
    nano_ros_node_register(
        NAME talker
        CLASS demo::Talker
        ENTITIES sub:std_msgs/msg/String:/chatter
        SOURCES src/dummy.cpp
        DEPLOY native)
endmacro()
