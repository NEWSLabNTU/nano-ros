# NanoRosFeatureImage.cmake -- which image a configure reads capability axes for.
#
# Issue 2700: a configure that builds one image (`-DNROS_IMAGE=<id>`) reads that
# image's own `features` / `node_interfaces` wherever the axes are read: the
# capability list (`nros config show`), the entity inventory, the sizing
# descriptor and the entry's codegen.
#
# Issue 2702 (ruled 2026-11-22): `nros build` gives every workspace image a
# cmake configure of its own and names it here as `-DNROS_FEATURE_IMAGE=<id>`.
# That is a narrower statement than `NROS_IMAGE`, which on a hand-written root
# also picks the image's deploy, its board facts and its leaf rows. Only the
# axes readers ask this helper.
include_guard(GLOBAL)

# _nros_feature_image_arg(<out>)
#
# `<out>` = `--image <id>` for the image whose axes this configure builds with,
# or empty for `[system]`'s: `NROS_FEATURE_IMAGE`, else `NROS_IMAGE`.
function(_nros_feature_image_arg _out)
    if(DEFINED NROS_FEATURE_IMAGE AND NOT "${NROS_FEATURE_IMAGE}" STREQUAL "")
        set(${_out} --image "${NROS_FEATURE_IMAGE}" PARENT_SCOPE)
    elseif(DEFINED NROS_IMAGE AND NOT "${NROS_IMAGE}" STREQUAL "")
        set(${_out} --image "${NROS_IMAGE}" PARENT_SCOPE)
    else()
        set(${_out} "" PARENT_SCOPE)
    endif()
endfunction()
