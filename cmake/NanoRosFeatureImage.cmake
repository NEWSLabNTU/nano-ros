# NanoRosFeatureImage.cmake -- which image a configure reads capability axes for.
#
# Issue 2700: a configure that builds one image (`-DNROS_IMAGE=<id>`) reads that
# image's own `features` / `node_interfaces` wherever the axes are read: the
# capability list (`nros config show`), the entity inventory, the sizing
# descriptor and the entry's codegen.
#
# Issue 2702 (ruled option 2): `nros build` gives an image whose own axes differ
# from `[system]`'s a cmake configure of its own, shared only with images whose
# axes equal it, and names it here as `-DNROS_FEATURE_IMAGE=<id>`. That is a
# narrower statement than `NROS_IMAGE`, which also picks the image's deploy, its
# board facts and its leaf rows, none of which a configure shared by several
# images may take from one of them. Only the axes readers ask this helper.
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
