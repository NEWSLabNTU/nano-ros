# PX4 module CMakeLists template (copy-out shape).
#
# Adapt for your module:
#   - MAIN is your entry-point name, matching the <name>_main symbol.
#   - List your sources under SRCS.
#   - BACKENDS names the RMW backends to register (see below).
#
# phase-325 W1.3 — this used to call px4_add_module() directly with hand-listed
# nano-ros include dirs, and its .cpp body was a comment reading "Replace this
# comment block with NodeBuilder / Publisher calls". A template that does not
# compile what it documents cannot go stale loudly: its include paths still said
# `packages/api/...` for headers that had moved, and nothing noticed because
# nothing ever built it.
#
# Now it uses the shared helper, which resolves the archives and include dirs and
# fails at CONFIGURE time when either is missing or moved.

include("$ENV{NROS_REPO_DIR}/integrations/px4/NanoRosPx4Module.cmake")

nros_px4_add_module(
	MODULE   modules__nano_ros_app
	MAIN     nano_ros_app

	# REQUIRED. The helper generates the strong nros_app_register_backends()
	# from this list, overriding nros-c's weak no-op. There is deliberately no
	# default: an empty hook links perfectly and registers nothing, and every
	# entity creation then fails at runtime with no backend.
	BACKENDS uorb

	SRCS
		${CMAKE_CURRENT_LIST_DIR}/nano_ros_app.cpp
)
