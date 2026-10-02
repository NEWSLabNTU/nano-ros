# Case `cmake_register_entities_valueless` — see ../CMakeLists.txt.
#
# The valueless ENTITIES form the old KEYWORDS_MISSING_VALUES arm covered.
# Must raise the tombstone.
set(VERDICT_PROJECT talker_pkg)
macro(verdict_body)
    nano_ros_node_register(
        NAME talker
        CLASS demo::Talker
        ENTITIES
        SOURCES src/dummy.cpp
        DEPLOY native)
endmacro()
