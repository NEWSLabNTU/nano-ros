# Issue 2189 -- the image `nros_system_generate()` bakes must be the image the
# module's Kconfig hook rendered (`nros_image_kconfig.cmake`).
#
# The hook runs inside `find_package(Zephyr)`, before the application's
# CMakeLists, so it cannot read the bringup argument of
# `nros_system_generate(<bringup>)`. It renders the image it was told about
# (`-DNROS_IMAGE_SYSTEM_DIR` + `-DNROS_IMAGE`), or the application's own
# `system.toml`, or nothing, and records which in two global properties. The
# bake runs later and resolves its image from `entry =`. A hand-written
# application whose image lives in a SIBLING bringup reached the bake and not
# Kconfig: its rmw, board snippets and knobs were silently replaced by Kconfig
# defaults while the bake read the image (measured on three rows, one building
# Cyclone while its image said zenoh). These two functions are the only place
# that sees both halves, so the disagreement is refused here.
#
# Refuse and name the fix, rather than have the hook search for the image
# that claims the application: the hook would need a scope to search, and a
# wrong guess applies another image's knobs with nothing said (the maintainer's
# choice, 2026-11-01). Functions only, include_guard'd.
include_guard(GLOBAL)

# nros_system_image_args(<out-var> <bringup-pkg> <bringup-dir> <entry-dir>)
#
# The image arguments for `nros codegen-system`. When the configure was TOLD
# its image, the bake is that image (`--target`), and the bringup argument must
# name the same directory: two spellings of one bringup that disagree are
# refused, not reconciled. Otherwise `--for-entry`, as before.
function(nros_system_image_args _out _bringup_pkg _bringup_dir _entry_dir)
    get_property(_rendered_dir GLOBAL PROPERTY NROS_IMAGE_RENDERED_DIR)
    get_property(_rendered_image GLOBAL PROPERTY NROS_IMAGE_RENDERED_IMAGE)
    get_filename_component(_bringup_real "${_bringup_dir}" REALPATH)
    set(_args --for-entry "${_entry_dir}")
    if(DEFINED NROS_IMAGE_SYSTEM_DIR AND NOT NROS_IMAGE_SYSTEM_DIR STREQUAL "")
        if(NOT _rendered_dir STREQUAL _bringup_real)
            message(FATAL_ERROR
                "nros_system_generate(${_bringup_pkg}): this configure was told its image "
                "lives in ${_rendered_dir} (-DNROS_IMAGE_SYSTEM_DIR=${NROS_IMAGE_SYSTEM_DIR}), "
                "and this call bakes ${_bringup_real}. Name one bringup in both places "
                "(issue 2189).")
        endif()
        set(_args --target "${_rendered_image}")
    endif()
    set(${_out} ${_args} PARENT_SCOPE)
endfunction()

# nros_system_check_image_agreement(<codegen-system stdout> <bringup-pkg> <bringup-dir>)
#
# `codegen-system` prints the image it resolved as `NROS_SYSTEM_IMAGE=<id>`
# (none when no image claims the entry, which its own NOTE reports). Refuse a
# bake whose image is not the one Kconfig rendered, and name the two arguments
# that make the hook render it.
function(nros_system_check_image_agreement _stdout _bringup_pkg _bringup_dir)
    set(_baked_image "")
    if(_stdout MATCHES "(^|\n)NROS_SYSTEM_IMAGE=([^\n]*)")
        set(_baked_image "${CMAKE_MATCH_2}")
    endif()
    if(_baked_image STREQUAL "")
        return()
    endif()
    get_property(_rendered_dir GLOBAL PROPERTY NROS_IMAGE_RENDERED_DIR)
    get_property(_rendered_image GLOBAL PROPERTY NROS_IMAGE_RENDERED_IMAGE)
    get_filename_component(_bringup_real "${_bringup_dir}" REALPATH)
    if(_rendered_dir STREQUAL _bringup_real AND _rendered_image STREQUAL _baked_image)
        return()
    endif()
    file(RELATIVE_PATH _rel "${APPLICATION_SOURCE_DIR}" "${_bringup_real}")
    if(_rendered_image STREQUAL "")
        string(CONCAT _what "this configure's Kconfig rendered no image, so the "
            "image's rmw, board snippets and knobs never reached it")
    else()
        string(CONCAT _what "this configure's Kconfig rendered image "
            "`${_rendered_image}` from ${_rendered_dir}")
    endif()
    message(FATAL_ERROR
        "nros_system_generate(${_bringup_pkg}): the bake resolved image `${_baked_image}` "
        "(${_bringup_real}/system.toml claims this application), but ${_what}. "
        "The module renders Kconfig before this CMakeLists runs, so tell it which "
        "image this is:\n"
        "  west build ... -- -DNROS_IMAGE_SYSTEM_DIR=${_rel} -DNROS_IMAGE=${_baked_image}\n"
        "or move the [image.${_baked_image}] block into a system.toml beside this "
        "application (issue 2189).")
endfunction()
