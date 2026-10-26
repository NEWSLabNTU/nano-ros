# NanoRosAmentTargetDeps.cmake — `ament_target_dependencies`, the one ament
# verb every nano-ros CMake package carries, Zephyr's included.
#
# Split out of NanoRosAmentSurface.cmake (phase-482 W3). The Zephyr arm of
# `nano_rosConfig.cmake` deliberately skips the full ament surface (it asserts
# NanoRos::NanoRosCpp, which a C-only Zephyr image does not define), and so a
# ported package's `ament_target_dependencies(<name> diagnostic_updater)` was an
# unknown command on Zephyr while the same line built on FreeRTOS. One file,
# included by both, keeps the two packages' build glue identical.
include_guard(GLOBAL)

# phase-488 — the platform link pass for an executable an ament CMakeLists
# builds, ONE spelling for every ament verb that reaches one. The pass emits
# the strong `nros_app_register_backends()` TU and the platform's link
# settings; an executable without it fails to link (`undefined reference to
# nros_app_register_backends`). Deferred to the end of the directory and run
# once per target (`nros_platform_link_app_deferred`, phase-287 W6), because
# `ament_target_dependencies` adds links after the target exists and NuttX's
# pass reads the finished link closure.
#
# Both arms need the platform overlay's own `nros_platform_link_app`. Zephyr
# has none (the kernel's `app` is linked by Zephyr), but it does load
# NanoRosEntry.cmake, which defines the deferred wrapper. Asking only for the
# wrapper scheduled a call to a command that does not exist, and the configure
# failed at the end of the directory with `Unknown CMake command
# "nros_platform_link_app"` (phase-518 W4, node 1 on Zephyr).
function(_nros_ament_link_app target)
    if(NOT COMMAND nros_platform_link_app)
        return()
    endif()
    if(COMMAND nros_platform_link_app_deferred)
        nros_platform_link_app_deferred(${target})
    else()
        get_property(_nal_done TARGET ${target} PROPERTY _NROS_PLATFORM_LINK_DONE)
        if(NOT _nal_done)
            set_property(TARGET ${target} PROPERTY _NROS_PLATFORM_LINK_DONE TRUE)
            nros_platform_link_app(${target})
        endif()
    endif()
endfunction()

function(ament_target_dependencies target)
    # Stock ROS 2 form: ament_target_dependencies(<target> rclcpp std_msgs …).
    # Each dep is a *package* whose `find_package(<dep>)` defined a target.
    # Wire only the deps the stubs actually create targets for (rclcpp +
    # rclcpp_components today); the rest are no-ops because nano-ros pulls in
    # the message + ROS surface through NanoRos::NanoRosCpp anyway.
    #
    # phase-482 W3 — on Zephyr `nano_ros_add_executable(<target> …)` compiles
    # the sources into the kernel's `app` and leaves `<target>` a placeholder
    # (`NROS_SOURCES_IN_TARGET`, cmake/NanoRosEntry.cmake). The dependencies
    # are for those sources, so they go where the sources went.
    set(_atd_into "${target}")
    if(TARGET ${target})
        get_target_property(_atd_sources_in ${target} NROS_SOURCES_IN_TARGET)
        if(_atd_sources_in AND TARGET ${_atd_sources_in})
            set(_atd_into "${_atd_sources_in}")
        endif()
    endif()
    #
    # phase-488 — upstream's leading `SYSTEM`, `INTERFACE` and `PUBLIC`
    # keywords are accepted. Upstream links with the PLAIN `target_link_libraries`
    # signature unless one is given, and this linked with `PRIVATE` always.
    # Either choice breaks somebody, because CMake refuses to mix the two
    # signatures on one target:
    #
    #   * a stock CMakeLists that also writes a plain `target_link_libraries`
    #     (ros2/demos `composition`: `manual_composition` links its components
    #     plain) fails against `PRIVATE`;
    #   * the four in-tree packages that write `target_link_libraries(<t> PRIVATE
    #     ...)` beside this call would fail against plain.
    #
    # So the dependency is appended to the target's link PROPERTIES, which is
    # what both signatures write, and neither signature is claimed. The
    # transitivity is upstream's: a library passes its dependencies on to its
    # consumers (plain or `PUBLIC`), `INTERFACE` passes them on only, and an
    # executable keeps them. The Zephyr redirect target above is the kernel's
    # `app`, which Zephyr links with keywords, so there the dependency stays a
    # `PRIVATE` keyword link.
    set(_atd_scope "")
    set(_atd_deps "")
    foreach(_arg IN LISTS ARGN)
        if(_arg STREQUAL "SYSTEM" AND NOT _atd_deps)
            continue()
        elseif((_arg STREQUAL "INTERFACE" OR _arg STREQUAL "PUBLIC") AND NOT _atd_deps)
            set(_atd_scope "${_arg}")
        else()
            list(APPEND _atd_deps "${_arg}")
        endif()
    endforeach()
    set(_atd_carries_interface FALSE)
    set(_atd_type "")
    if(TARGET ${_atd_into})
        get_target_property(_atd_type ${_atd_into} TYPE)
        if(_atd_type MATCHES "LIBRARY$" AND NOT _atd_type STREQUAL "INTERFACE_LIBRARY")
            set(_atd_carries_interface TRUE)
        endif()
    endif()
    # phase-518 — a message package `find_package(nano_ros)` only VALIDATED.
    # That road leaves generation to `nano_ros_add_*`, which knows the leaf's
    # language; a stock package's own `add_library` / `add_executable` never
    # reaches it, so its first `#include <std_msgs/msg/string.hpp>` failed on
    # FreeRTOS and Zephyr while the same package built on posix. The consumer
    # here is a target, so its language is known: one with C++ sources gets the
    # package generated in C++ now. A C target does not, which keeps the reason
    # for validating (no C++ interface library in a C leaf's scope).
    set(_atd_cxx FALSE)
    if(TARGET ${target})
        get_target_property(_atd_srcs ${target} SOURCES)
        foreach(_s IN LISTS _atd_srcs)
            if(_s MATCHES "\\.(cc|cpp|cxx|C)$")
                set(_atd_cxx TRUE)
            endif()
        endforeach()
    endif()
    foreach(_dep IN LISTS _atd_deps)
        if(NOT TARGET ${_dep}::${_dep} AND _atd_cxx AND NROS_FIND_PACKAGE_VALIDATE_ONLY
           AND COMMAND _nros_require_cpp_msg_package)
            _nros_require_cpp_msg_package(${_dep})
        endif()
        if(NOT TARGET ${_dep}::${_dep})
            continue()
        endif()
        if(NOT _atd_into STREQUAL target)
            target_link_libraries(${_atd_into} PRIVATE ${_dep}::${_dep})
            continue()
        endif()
        if(NOT _atd_scope STREQUAL "INTERFACE")
            set_property(TARGET ${_atd_into} APPEND PROPERTY LINK_LIBRARIES ${_dep}::${_dep})
        endif()
        if(_atd_scope STREQUAL "INTERFACE" OR _atd_carries_interface)
            set_property(TARGET ${_atd_into} APPEND PROPERTY
                INTERFACE_LINK_LIBRARIES ${_dep}::${_dep})
        endif()
        # phase-488 — a plain `add_executable` that takes rclcpp this way
        # (ros2/demos `manual_composition`) is an image, and never reached the
        # platform link pass the ament verbs run.
        if(_atd_type STREQUAL "EXECUTABLE" AND _dep MATCHES "^rclcpp(_components|_lifecycle|_action)?$")
            _nros_ament_link_app(${_atd_into})
        endif()
    endforeach()
endfunction()
