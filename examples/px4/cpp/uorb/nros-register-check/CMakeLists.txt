# Phase 115.K.4.5 — PX4-SITL validation module for nros-rmw-uorb.
#
# Compiles the C++ uORB RMW backend (`packages/px4/nros-rmw-uorb/`)
# inline into a PX4 module so that:
#   - The core data-plane sources see real `<uORB/uORB.h>` declarations.
#   - The push-wake glue (`px4_callback_glue.cpp`) sees real
#     `<uORB/SubscriptionCallback.hpp>` + `WorkQueueManager.hpp` and
#     compiles against the per-board Kconfig output
#     (`px4_boardconfig.h`) that only exists inside a `px4_add_module()`
#     build context.
#   - The module's `task_main` actually invokes
#     `nros_rmw_uorb_register()` so the linker resolves every entry
#     point the cffi adapter dispatches to.
#
# This is the only build path that exercises the K.4.2-sub-push glue
# end-to-end against real PX4 sources; the standalone smoke under
# `packages/px4/nros-rmw-uorb/tests/register_smoke.cpp` covers the
# pure data-plane via a mock uORB ABI.

set(NROS_RMW_UORB_CPP_DIR
    ${CMAKE_CURRENT_LIST_DIR}/../../../../../packages/px4/nros-rmw-uorb)
set(NROS_RMW_CFFI_INCLUDE_DIR
    ${CMAKE_CURRENT_LIST_DIR}/../../../../../packages/core/nros-rmw-abi/include)

if(NOT EXISTS "${NROS_RMW_UORB_CPP_DIR}/src/vtable.cpp")
    message(FATAL_ERROR
        "nros-rmw-uorb sources not found at NROS_RMW_UORB_CPP_DIR='${NROS_RMW_UORB_CPP_DIR}'")
endif()
if(NOT EXISTS "${NROS_RMW_CFFI_INCLUDE_DIR}/nros/rmw_vtable.h")
    message(FATAL_ERROR
        "nros-rmw-cffi headers not found at NROS_RMW_CFFI_INCLUDE_DIR='${NROS_RMW_CFFI_INCLUDE_DIR}'")
endif()

px4_add_module(
    MODULE         modules__nros_register_check
    MAIN           nros_register_check
    COMPILE_FLAGS
        # Flip uorb_abi.hpp to "#include <uORB/uORB.h>" mode.
        -DNROS_RMW_UORB_USE_PX4_HEADER=1
    INCLUDES
        ${CMAKE_CURRENT_LIST_DIR}
        ${NROS_RMW_UORB_CPP_DIR}/include
        ${NROS_RMW_UORB_CPP_DIR}/src
        ${NROS_RMW_CFFI_INCLUDE_DIR}
    SRCS
        ${CMAKE_CURRENT_LIST_DIR}/nros_register_check.cpp
        # phase-244 D5 — SITL-only weak `nros_rmw_cffi_register` link stub,
        # hoisted out of the agnostic module source (real builds override it
        # with the Rust `nros-rmw-cffi` strong symbol).
        ${CMAKE_CURRENT_LIST_DIR}/sitl_register_stub.c
        ${NROS_RMW_UORB_CPP_DIR}/src/vtable.cpp
        ${NROS_RMW_UORB_CPP_DIR}/src/session.cpp
        ${NROS_RMW_UORB_CPP_DIR}/src/publisher.cpp
        ${NROS_RMW_UORB_CPP_DIR}/src/subscriber.cpp
        ${NROS_RMW_UORB_CPP_DIR}/src/service.cpp
        ${NROS_RMW_UORB_CPP_DIR}/src/topic_registry.cpp
        ${NROS_RMW_UORB_CPP_DIR}/src/callback_default.cpp
        ${NROS_RMW_UORB_CPP_DIR}/src/px4_callback_glue.cpp
)
# `px4_work_queue` is the lib that provides
# `SubscriptionCallbackWorkItem` + `WorkQueueManager`. External
# modules can't list it in `DEPENDS` (the target doesn't exist at
# the point PX4 walks `EXTERNAL_MODULES_LOCATION`). Wire the
# dependency post-hoc — `add_dependencies` is order-agnostic, and the
# symbols resolve at the final PX4 binary link.
if(TARGET modules__nros_register_check AND TARGET px4_work_queue)
    add_dependencies(modules__nros_register_check px4_work_queue)
endif()
