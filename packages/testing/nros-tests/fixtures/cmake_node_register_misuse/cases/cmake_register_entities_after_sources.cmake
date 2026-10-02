# Case `cmake_register_entities_after_sources` — see ../CMakeLists.txt.
#
# Retired ENTITIES after a multi-value keyword — swallowed into SOURCES if
# nothing refuses it. Must raise the ENTITIES tombstone.
set(VERDICT_PROJECT talker_pkg)
macro(verdict_body)
    nano_ros_node_register(
        NAME talker
        CLASS demo::Talker
        SOURCES src/dummy.cpp
        ENTITIES sub:std_msgs/msg/String:/chatter
        DEPLOY native)
endmacro()
