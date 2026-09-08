# Find-stub for rclcpp — Phase 209.B (NrosRclcppCompat) + Phase 210.A.4 update.
#
# Defines the `rclcpp::rclcpp` IMPORTED INTERFACE target that ROS 2 cmake
# typically links against (`target_link_libraries(my_target rclcpp::rclcpp)`),
# transparently forwarding to NanoRos::NanoRosCpp so the link resolves to the
# nano-ros surface.
#
# Phase 210.A.4 — also publishes the rclcpp_compat shim header path +
# force-include flags on the imported target so a stock `add_executable() +
# target_link_libraries(... rclcpp::rclcpp)` consumer (no ament_auto_*
# routing) gets the source-compat layer automatically. The 209.B
# ament_auto_* shims still apply the same hookup via _nros_compat_apply_
# force_includes; this stub is the second entry point that catches
# upstream-style call sites.
if(NOT TARGET rclcpp::rclcpp)
    add_library(rclcpp::rclcpp INTERFACE IMPORTED)
    if(TARGET NanoRos::NanoRosCpp)
        target_link_libraries(rclcpp::rclcpp INTERFACE NanoRos::NanoRosCpp)
    endif()
    # NrosRclcppCompat lives at `../include/` relative to this stub dir.
    get_filename_component(_nros_compat_inc_dir "${CMAKE_CURRENT_LIST_DIR}/../include" ABSOLUTE)
    target_include_directories(rclcpp::rclcpp INTERFACE "${_nros_compat_inc_dir}")
    # Issue 1239 — `nros/rclcpp_compat.hpp` was force-included here too, and
    # `f35f0b878` (phase-417 stage 6 step A) DELETED that header when the
    # `rclcpp::` spellings moved into `<nros/nros.hpp>` itself. A force-include
    # of a missing header kills the TU before it reads a line of its own source,
    # so every upstream-style `find_package(rclcpp)` consumer failed with
    # `fatal error: nros/rclcpp_compat.hpp: No such file or directory`. The
    # sibling below is NOT dead: `rclcpp_components_compat.hpp` still exists and
    # is what `NrosRclcppCompat.cmake:103` force-includes, which is why the
    # ament_auto_* entry point kept working and this one did not.
    target_compile_options(rclcpp::rclcpp INTERFACE
        "$<$<COMPILE_LANGUAGE:CXX>:SHELL:-include nros/rclcpp_components_compat.hpp>"
    )
    # Issue 1239, second half — ASK for the std surface, exactly as
    # `_nros_compat_apply_force_includes` does per target since phase-438 W2.
    # It was added there and not here, and the two are meant to be equivalent
    # entry points: that one serves `ament_auto_add_*`, this one serves a stock
    # `find_package(rclcpp)` + `ament_target_dependencies`, and
    # `ament_target_dependencies` links this INTERFACE target WITHOUT calling
    # the per-target helper. So a consumer written the upstream way got the
    # include dir and the force-include but not the definition, and every
    # `rclcpp::`-spelled name it reached was absent.
    #
    # INTERFACE and not PRIVATE, because the flag has to reach whatever links
    # this target; that is the only mechanism the stub has.
    target_compile_definitions(rclcpp::rclcpp INTERFACE NROS_CPP_STD=1)
endif()
set(rclcpp_FOUND TRUE)
