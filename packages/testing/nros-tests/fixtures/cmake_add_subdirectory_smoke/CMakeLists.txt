cmake_minimum_required(VERSION 3.22)
project(smoke C)

# Build-stage fixture for issue 0041 (no compilation inside tests). Smoke-tests
# that a user project can `add_subdirectory(nano-ros)` + link NanoRos::NanoRos.
# The repo root is 5 levels up from this fixture dir; out-of-tree
# add_subdirectory needs the explicit binary dir (2nd arg).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)

get_filename_component(NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." ABSOLUTE)
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)

add_executable(smoke main.c)
target_link_libraries(smoke PRIVATE NanoRos::NanoRos)
nros_platform_link_app(smoke)

# issue 0719 — an image path that does not go through `nano_ros_entry()` still
# has to apply the image's cross-cutting facts, so it calls the shared applier.
#
# Today that fact is the panic policy; the point of routing through ONE function
# is that whatever is added to it next arrives here too. The two paths that
# already did this (#0688's board seam, #0700's ESP-IDF shim) each got it after
# a build broke with `#[panic_handler] required` four crates from the cause.
#
# The include is guarded because these files are reached several ways
# (`find_package(nano_ros)`, `add_subdirectory`, an ament overlay) and not all
# of them have pulled `NanoRosEntry.cmake` first.
if(NOT COMMAND nros_apply_panic_policy)
    include("${NANO_ROS_ROOT}/cmake/NanoRosFeatureSet.cmake")
endif()
nros_apply_panic_policy(platform "cmake_add_subdirectory_smoke fixture")
