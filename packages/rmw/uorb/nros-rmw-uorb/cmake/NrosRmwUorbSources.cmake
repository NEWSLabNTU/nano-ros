# NrosRmwUorbSources.cmake — the ONE list of nros-rmw-uorb's translation units.
#
# Three builds compile this backend from source, and each used to carry its own
# list: this project's `add_library`, `nros_px4_add_module(BACKENDS uorb)`
# (integrations/px4/NanoRosPx4Module.cmake) and the register-check gate
# (packages/testing/nros-px4-register-check). The helper's own comment predicted
# the failure — "the copies drift the moment the backend gains a file" — and
# issue 1437 added `src/qos.cpp` to the first list only. Every PX4 image then
# failed to LINK (`undefined reference to nros_rmw_uorb::qos_admit`), and no
# merge-gating lane builds a PX4 image, so it surfaced only when phase-325 W3.4
# built the bridge.
#
# Including this file sets, in the CALLER's scope (a function's scope is fine —
# the values are used in the same call):
#
#   NROS_RMW_UORB_SOURCES           absolute paths of the backend proper
#   NROS_RMW_UORB_PX4_GLUE_SOURCES  the push-wake glue, compiled only inside a
#                                   real PX4 build (`<px4_boardconfig.h>`)
#
# Paths are absolute so a consumer outside this directory needs no prefix.

set(_nros_rmw_uorb_src_dir "${CMAKE_CURRENT_LIST_DIR}/../src")
get_filename_component(_nros_rmw_uorb_src_dir "${_nros_rmw_uorb_src_dir}" ABSOLUTE)

set(NROS_RMW_UORB_SOURCES
    "${_nros_rmw_uorb_src_dir}/vtable.cpp"
    "${_nros_rmw_uorb_src_dir}/session.cpp"
    "${_nros_rmw_uorb_src_dir}/publisher.cpp"
    "${_nros_rmw_uorb_src_dir}/subscriber.cpp"
    "${_nros_rmw_uorb_src_dir}/service.cpp"
    "${_nros_rmw_uorb_src_dir}/topic_registry.cpp"
    "${_nros_rmw_uorb_src_dir}/qos.cpp"
    "${_nros_rmw_uorb_src_dir}/callback_default.cpp"
)
set(NROS_RMW_UORB_PX4_GLUE_SOURCES
    "${_nros_rmw_uorb_src_dir}/px4_callback_glue.cpp"
)
unset(_nros_rmw_uorb_src_dir)
