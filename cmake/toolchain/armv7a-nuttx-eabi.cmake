# cmake/toolchain/armv7a-nuttx-eabi.cmake -- DEPRECATED forwarding file.
#
# Renamed to armv7a-nuttx-eabihf.cmake (phase-550 W12), after the hard-float
# triple it has always set. This file stays because CMake records the toolchain
# file's path in <build>/CMakeFiles/<version>/CMakeSystem.cmake at the first
# configure and includes that recorded path on every later configure, even
# when -DCMAKE_TOOLCHAIN_FILE names a new one. A build dir configured before
# the rename therefore keeps including this path; deleting it strands every
# such dir at "include could not find requested file". New builds name
# armv7a-nuttx-eabihf.cmake.
include("${CMAKE_CURRENT_LIST_DIR}/armv7a-nuttx-eabihf.cmake")
