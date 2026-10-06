cmake_minimum_required(VERSION 3.22)
project(image_env_probe_c LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# issue 1712 — a C leaf whose `system.toml` states knobs through
# `[image.native] env` (RFC-0049's APP rung). Nothing here names them: the
# cmake road must carry the image's rows to every cargo build it drives.
find_package(nano_ros REQUIRED)

nano_ros_add_executable(image_env_probe_c src/main.c)
install(TARGETS image_env_probe_c DESTINATION lib/${PROJECT_NAME})

ament_package()
