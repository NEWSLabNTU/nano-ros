# Case `cmake_register_without_entities` — see ../CMakeLists.txt.
#
# NEGATIVE CONTROL: no ENTITIES argument, so the tombstone must NOT fire. The
# configure may still fail for its own reasons (no board is staged).
set(VERDICT_PROJECT talker_pkg)
macro(verdict_body)
    nano_ros_node_register(
        NAME talker
        CLASS demo::Talker
        SOURCES src/dummy.cpp
        DEPLOY native)
endmacro()
