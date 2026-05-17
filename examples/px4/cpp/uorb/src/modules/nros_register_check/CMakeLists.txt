# PX4 EXTERNAL_MODULES_LOCATION shim — PX4 requires every external
# module to live at `${EXTERNAL_MODULES_LOCATION}/src/modules/<name>/`.
# The real CMakeLists for this example lives one level up at
# `examples/px4/cpp/uorb/nros-register-check/` so the example tree
# follows the canonical `<plat>/<lang>/<rmw>/<example>` shape (Phase
# 131.C.2). This shim re-exports the hoisted file unchanged.

include(${CMAKE_CURRENT_LIST_DIR}/../../../nros-register-check/CMakeLists.txt)
