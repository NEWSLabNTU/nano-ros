# CMake module for `fixture-r5f-tiarmclang`: the same board as
# `fixture-r5f-freertos`, built with TI Arm Clang (phase-575 W3). One module
# serves both; it picks the linker's input by CMAKE_C_COMPILER_ID.
include("${CMAKE_CURRENT_LIST_DIR}/nano-ros-board-fixture-r5f-freertos.cmake")
