cmake_minimum_required(VERSION 3.22)
project(c_fib_server_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(example_interfaces REQUIRED)

# phase-263 A4 (actions, C projection) — typed component using the generated
# example_interfaces/action/Fibonacci C bindings (deserialize goal / serialize result). The
# verb generates + links the `example_interfaces__nano_ros_c` interface lib from package.xml.
nano_ros_add_node(fib_server CLASS c_fib_server_pkg::FibServer TYPED src/FibServer.c)
