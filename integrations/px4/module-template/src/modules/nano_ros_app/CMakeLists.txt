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

# Resolve the nano-ros SDK root — RFC-0103 D6 (amended 2026-10-12). This
# block is the ONE leaf spelling, byte-identical in every leaf that must know
# the root before it can include anything (a copy-out leaf has no repo file to
# include yet); `check-repo-dir-readers` refuses any `$ENV{NROS_REPO_DIR}`.
#   1. explicit: -DNROS_REPO_DIR=<path> or -Dnano_ros_ROOT=<path>;
#   2. ask `nros sdk-root`. It walks up from THIS leaf first, so an in-tree
#      leaf answers its own checkout; anywhere else the binary answers with
#      its own SDK root (a release's share/nano-ros, or the checkout an in-tree
#      `nros` was built in).
# Never the environment: an exported path is what a linked worktree inherits
# from its parent (issue 1280).
if(NOT NROS_REPO_DIR)
    if(nano_ros_ROOT)
        set(NROS_REPO_DIR "${nano_ros_ROOT}")
    else()
        find_program(NROS_TOOLCHAIN_EXECUTABLE nros)
        if(NROS_TOOLCHAIN_EXECUTABLE)
            execute_process(
                COMMAND "${NROS_TOOLCHAIN_EXECUTABLE}" sdk-root
                WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
                OUTPUT_VARIABLE NROS_REPO_DIR
                OUTPUT_STRIP_TRAILING_WHITESPACE
                RESULT_VARIABLE _nros_sdk_root_rc)
            if(NOT _nros_sdk_root_rc EQUAL 0)
                # An `nros` too old to know the verb exits non-zero: no answer,
                # rather than clap's usage text as a path.
                set(NROS_REPO_DIR "")
            endif()
        endif()
    endif()
    if(NOT NROS_REPO_DIR)
        message(FATAL_ERROR
            "nano-ros: cannot locate the nano-ros SDK root.\n"
            "  Pass -DNROS_REPO_DIR=<path> (or -Dnano_ros_ROOT=<path>), or put\n"
            "  `nros` on PATH — `nros sdk-root` answers this.")
    endif()
endif()
include("${NROS_REPO_DIR}/integrations/px4/NanoRosPx4Module.cmake")

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
