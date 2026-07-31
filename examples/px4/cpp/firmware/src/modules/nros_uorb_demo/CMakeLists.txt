# nano-ros uORB interop example (phase-325 W2).
#
# One call. The helper resolves libnros_cpp.a + the posix platform shim, prepends
# the per-build generated headers, generates the strong
# nros_app_register_backends() from BACKENDS, and contributes the uORB backend's
# own sources. See integrations/px4/NanoRosPx4Module.cmake.

include("$ENV{NROS_REPO_DIR}/integrations/px4/NanoRosPx4Module.cmake")

nros_px4_add_module(
	MODULE   modules__nros_uorb_demo
	MAIN     nros_uorb_demo
	BACKENDS uorb
	SRCS
		${CMAKE_CURRENT_LIST_DIR}/NrosUorbDemo.cpp
)
