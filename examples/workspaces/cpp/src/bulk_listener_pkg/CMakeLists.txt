cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project(bulk_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Declarative Node pkg shape, as listener_pkg: the Entry pkg owns main().
nano_ros_auto_add_library(bulk_listener_lib STATIC src/BulkListener.cpp)

nros_components_register_node(bulk_listener_lib
    PLUGIN bulk_listener_pkg::BulkListener
    EXECUTABLE bulk_listener
    SHAPE configure
)

# Guarded as in listener_pkg: on Zephyr the generated interface is
# whole-archived into `app` rather than exposed as a lib.
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(bulk_listener_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()
