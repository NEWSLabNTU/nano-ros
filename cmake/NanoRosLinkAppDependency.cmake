# NanoRosLinkAppDependency.cmake — link an image without claiming a signature
# (phase-518).
include_guard(GLOBAL)

# nros_link_app_dependency(<target> <lib>...)
#
# Link <lib> into an APP target the user created, without claiming a
# `target_link_libraries` signature. CMake refuses to mix the plain and the
# keyword signature on one target, and the user's CMakeLists has already chosen
# one: ros2/demos `composition` links `manual_composition` plain, so the
# deferred platform link's `PRIVATE`, and then `nano_ros_entry`'s own runtime
# link, failed the FreeRTOS configure. Appending to
# `LINK_LIBRARIES` is what both signatures write for an executable, so either
# choice of the user's stays legal. Used by every step that links an image the
# user's CMakeLists also links: `nano_ros_entry`'s runtime link, the generated
# interface libraries (`_nros_link`), and every platform and board app-link step
# (`nros_platform_link_app`, `nros_board_link_app`, `nano_ros_link_rmw`).
function(nros_link_app_dependency target)
    set_property(TARGET ${target} APPEND PROPERTY LINK_LIBRARIES ${ARGN})
endfunction()
