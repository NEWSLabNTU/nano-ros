# NanoRosImageRmw.cmake — which RMW backend this image is, and whether a
# backend-specific knob is this image's to report.
#
# Functions only, include_guard'd, so it is safe to include from inside a
# function frame and from `cmake -P` scripts.

include_guard(GLOBAL)

# nros_resolve_image_rmw(<out_var>)
#
# WHICH BACKEND this image is, as far as this configure can tell. Empty when
# nothing in it has chosen one yet, which is a legitimate state — so this
# never FATALs, unlike `nano_ros_link_rmw`.
#
# Same resolution ORDER as `nano_ros_link_rmw`'s fallback list
# ("NROS_DEFAULT_RMW;NROS_RMW"), then KCONFIG.
#
# THE KCONFIG ARM IS NOT DECORATION. A west Zephyr image selects its backend
# with `CONFIG_NROS_RMW_<X>=y` and sets NEITHER cmake variable: measured on
# `build-ws-cpp-entry-cyclonedds`, whose CMakeCache has no `NROS_RMW` at all
# while its `.config` carries `CONFIG_NROS_RMW_CYCLONEDDS=y`. Without it the
# user-config bake was skipped and the image linked Cyclone (`ddsi_` x14283)
# carrying none of the user's bytes — configure clean, compile clean, silently
# unconfigured.
#
# ONE FUNCTION. Its askers are the entry's user-config bake, phase-454 W14's
# sizing descriptor and (issue 2093) the message-bound derivation's
# backend-specific report lines, and the mapping is already mirrored by
# `nros_system_generate`. A second copy is how two wirings come to disagree
# about which backend an image is. It lived in `NanoRosEntry.cmake` as
# `_nros_entry_resolve_rmw` until the third asker sat outside that file.
function(nros_resolve_image_rmw _out_var)
    set(_rmw "")
    if(DEFINED NROS_DEFAULT_RMW AND NOT "${NROS_DEFAULT_RMW}" STREQUAL "")
        set(_rmw "${NROS_DEFAULT_RMW}")
    elseif(DEFINED NROS_RMW AND NOT "${NROS_RMW}" STREQUAL "")
        set(_rmw "${NROS_RMW}")
    elseif(CONFIG_NROS_RMW_CYCLONEDDS)
        set(_rmw "cyclonedds")
    elseif(CONFIG_NROS_RMW_XRCE)
        set(_rmw "xrce")
    elseif(CONFIG_NROS_RMW_ZENOH)
        set(_rmw "zenoh")
    endif()
    set(${_out_var} "${_rmw}" PARENT_SCOPE)
endfunction()

# nros_image_reports_backend_knob(<backend> <out_var>)
#
# Issue 2093 — TRUE when a knob that belongs to <backend> (`zenoh`, `xrce`,
# `cyclonedds`) is this image's to REPORT on: the image IS that backend, or
# this configure has not said which backend it is.
#
# The derivations that feed such a knob are backend-agnostic on purpose — one
# inventory feeds every backend, and an unread `NROS_DERIVED_*` value costs
# nothing — so they keep running for every image. What must not happen is a
# STATUS line telling a Cyclone image's author that `NROS_ZENOH_TL_RETAIN_BYTES`
# was not derived: it names a knob nothing in their image reads, and it reads
# as a fault to fix. Unknown keeps the line, because hiding a report from the
# image it DOES apply to is the worse error of the two.
function(nros_image_reports_backend_knob _backend _out_var)
    nros_resolve_image_rmw(_rmw)
    if(_rmw STREQUAL "" OR _rmw STREQUAL "${_backend}")
        set(${_out_var} TRUE PARENT_SCOPE)
    else()
        set(${_out_var} FALSE PARENT_SCOPE)
    endif()
endfunction()
