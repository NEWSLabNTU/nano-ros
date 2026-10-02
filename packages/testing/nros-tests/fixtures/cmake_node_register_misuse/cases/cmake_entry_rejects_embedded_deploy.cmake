# Case `cmake_entry_rejects_embedded_deploy` — see ../CMakeLists.txt.
#
# An embedded DEPLOY on an Entry (`nano_ros_entry`, the live spelling of the
# retired `nano_ros_application` shim) — must be rejected.
set(VERDICT_PROJECT my_app)
macro(verdict_body)
    nano_ros_entry(
        NAME my_app
        SOURCES src/dummy.cpp
        DEPLOY native zephyr)
endmacro()
