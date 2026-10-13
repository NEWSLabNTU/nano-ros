# rclcpp-compat smoke — Phase 209 MVP-quartet integration template.
#
# Pure ament_cmake_auto + rclcpp_components shape, mostly verbatim from what a
# ROS 2 user would write. The only nano-ros-specific lines are:
#   1) the platform / rmw selection + `add_subdirectory(<nano-ros>)`,
#   2) the `include(<nano-ros>/cmake/NanoRosAmentSurface.cmake)` one-liner,
#   3) the `nros_find_interfaces()` codegen call (package.xml drives the msg deps).
# Everything below those is unmodified `find_package + ament_auto_*`.

cmake_minimum_required(VERSION 3.22)
project(rclcpp_compat_smoke LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# 1) Pull nano-ros (provides NanoRos::NanoRos, NanoRos::NanoRosCpp + the codegen
#    function). Five levels up: examples/templates/rclcpp-compat-smoke/.
set(NROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW (zenoh|cyclonedds|xrce).")
set(NROS_RMW "${NROS_RMW}")
# Resolve the nano-ros SDK root — RFC-0103 D6 (amended 2026-10-12). This
# block is the ONE leaf spelling, byte-identical in every leaf that must know
# the root before it can include anything (a copy-out leaf has no repo file to
# include yet); `check-repo-dir-readers` refuses any `$ENV{NROS_REPO_DIR}`.
#   1. explicit: -DNROS_REPO_DIR=<path> or -Dnano_ros_ROOT=<path>;
#   2. ask `nros sdk-root`. It walks up from THIS leaf first, so an in-tree
#      leaf answers its own checkout; anywhere else the binary answers with
#      its own SDK root (a release's share/nano-ros, or the checkout an in-tree
#      `nros` was built in).
# Never the environment: an exported path is what a linked worktree inherits
# from its parent (issue 1280).
if(NOT NROS_REPO_DIR)
    if(nano_ros_ROOT)
        set(NROS_REPO_DIR "${nano_ros_ROOT}")
    else()
        # host-tool-path-ok: no SDK root was given, so this is a user project; its installed `nros` is on PATH.
        find_program(NROS_TOOLCHAIN_EXECUTABLE nros)
        if(NROS_TOOLCHAIN_EXECUTABLE)
            execute_process(
                COMMAND "${NROS_TOOLCHAIN_EXECUTABLE}" sdk-root
                WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
                OUTPUT_VARIABLE NROS_REPO_DIR
                OUTPUT_STRIP_TRAILING_WHITESPACE
                RESULT_VARIABLE _nros_sdk_root_rc)
            if(NOT _nros_sdk_root_rc EQUAL 0)
                # An `nros` too old to know the verb exits non-zero: no answer,
                # rather than clap's usage text as a path.
                set(NROS_REPO_DIR "")
            endif()
        endif()
    endif()
    if(NOT NROS_REPO_DIR)
        message(FATAL_ERROR
            "nano-ros: cannot locate the nano-ros SDK root.\n"
            "  Pass -DNROS_REPO_DIR=<path> (or -Dnano_ros_ROOT=<path>), or put\n"
            "  `nros` on PATH — `nros sdk-root` answers this.")
    endif()
endif()
add_subdirectory("${NROS_REPO_DIR}" nano_ros)

# 2) The ament / rclcpp surface: rclcpp / ament_cmake_auto / rclcpp_components /
#    diagnostic_updater. Everything below mirrors a stock ROS 2 CMakeLists.
include("${NROS_REPO_DIR}/cmake/NanoRosAmentSurface.cmake")

# 3) Generate the C++ bindings for the message packages this node uses.
#    package.xml is the single source of truth for the msg deps;
#    nros_find_interfaces() resolves the transitive closure (builtin_interfaces,
#    std_msgs, geometry_msgs, diagnostic_msgs) from its <depend> entries.
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# --- below: unmodified stock-ROS-2 ament_cmake_auto pattern ---

find_package(ament_cmake_auto REQUIRED)
find_package(rclcpp REQUIRED)
find_package(diagnostic_updater REQUIRED)
ament_auto_find_build_dependencies()

ament_auto_add_executable(rclcpp_compat_smoke src/talker.cpp)
ament_target_dependencies(rclcpp_compat_smoke rclcpp diagnostic_updater)
target_link_libraries(rclcpp_compat_smoke PRIVATE
    std_msgs__nano_ros_cpp
    diagnostic_msgs__nano_ros_cpp)


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
    include("${NROS_REPO_DIR}/cmake/NanoRosFeatureSet.cmake")
endif()
nros_apply_panic_policy(platform "examples/templates/rclcpp-compat-smoke")

ament_auto_package()
