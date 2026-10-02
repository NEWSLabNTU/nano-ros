# Case `cmake_register_unqualified_class` — see ../CMakeLists.txt.
#
# RFC-0057 D2: CLASS must be a namespace-QUALIFIED name — must FATAL_ERROR.
set(VERDICT_PROJECT talker_pkg)
macro(verdict_body)
    nano_ros_node_register(
        NAME talker
        CLASS Talker
        SOURCES src/dummy.cpp
        DEPLOY native)
endmacro()
