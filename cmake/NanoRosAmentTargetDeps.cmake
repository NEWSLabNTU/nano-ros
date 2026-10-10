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
    foreach(_dep IN LISTS ARGN)
        if(TARGET ${_dep}::${_dep})
            target_link_libraries(${_atd_into} PRIVATE ${_dep}::${_dep})
        endif()
    endforeach()
endfunction()
