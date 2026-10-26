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
#
# For a STATIC library this must also do the other half of what
# `target_link_libraries(<t> PRIVATE <lib>)` does: record `$<LINK_ONLY:<lib>>`
# in INTERFACE_LINK_LIBRARIES. A static library links nothing itself, so a
# dependency reaches the final link line only through that property. Zephyr's
# `app` is a static library. Without the interface half (issue 2095, from
# `8608f29c95`), `libnros_cpp.a` stopped being one of `app`'s own link items.
# It was emitted right after the component library that depends on it, ahead
# of the whole-archived message-FFI staticlib, and compiler-builtins'
# `__popcountdi2` was then defined twice in every native_sim C++ image.
function(nros_link_app_dependency target)
    set_property(TARGET ${target} APPEND PROPERTY LINK_LIBRARIES ${ARGN})
    get_target_property(_nlad_type ${target} TYPE)
    if(_nlad_type STREQUAL "STATIC_LIBRARY")
        foreach(_nlad_lib IN LISTS ARGN)
            set_property(TARGET ${target} APPEND PROPERTY
                INTERFACE_LINK_LIBRARIES "$<LINK_ONLY:${_nlad_lib}>")
        endforeach()
    endif()
endfunction()
