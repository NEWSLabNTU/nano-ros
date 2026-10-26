# NanoRosSecondStaticlib.cmake — a SECOND Rust staticlib in one image exports
# nothing but its own surface strongly.
#
# Issue 2092. A cargo `staticlib` bundles its whole closure, and on a host
# triple that closure includes `compiler_builtins` built with its `c` feature:
# 34 compiler-rt intrinsics compiled from C (`__popcountdi2`, `__cmpdi2`,
# `__muldc3`, ...) as ordinary STRONG globals, one archive member each. Fat LTO
# internalizes every Rust symbol of the archive and cannot touch these, because
# they are not Rust.
#
# A C++ Zephyr image links two such archives: the runtime (`libnros_cpp.a`,
# scanned lazily) and the image's message-FFI glue
# (`libnano_ros_cpp_ffi_image.a`, WHOLE-ARCHIVED — issue 0056, and one per image
# since issue 1645). The whole-archive forces all 34 in. If any C object ahead
# of the runtime on the line references one of them — Zephyr's own
# `lib/utils/bitarray.c` calls `__popcountdi2` — ld pulls that member out of the
# runtime first, and the whole-archive then defines it again:
#
#     libnano_ros_cpp_ffi_image.a(…-popcountdi2.o): multiple definition of
#     `__popcountdi2'; libnros_cpp.a(…-popcountdi2.o): first defined here
#
# Which intrinsic collides depends only on which ones the image's C happens to
# call, so fixing `__popcountdi2` by name would leave the other 33 armed.
#
# The rule: such an archive keeps its OWN surface (a glob, e.g. `nros_cpp_*`)
# strong and WEAKENS every other global it defines. A weak definition never
# conflicts: the runtime's strong copy wins when ld pulled it, and the weak one
# serves when nothing else defines the name. The copies are the same compiler-rt
# source from the same toolchain, so which one wins is not observable. The
# surface stays strong on purpose, so a genuine double definition of an FFI
# symbol is still a link error rather than a silent pick.
#
# Not a change of WHICH archives link: both still do, in the same order. The
# end state issue 1645 named — one Rust staticlib per image — would remove the
# second archive altogether; this is what makes two of them sound meanwhile.
#
# Usage (inside the custom command that produces the archive, AFTER cargo):
#
#     nros_second_staticlib_weaken_command(_cmd
#         ARCHIVE "${_ffi_lib}" KEEP "nros_cpp_*")
#     add_custom_command(OUTPUT "${_ffi_lib}"
#         COMMAND <cargo ...>
#         COMMAND ${_cmd}
#         ...)
#
# Idempotent (weakening a weak symbol is a no-op), so a re-run after a fresh
# cargo — which may leave the uplifted archive as it was — is harmless.

include_guard(GLOBAL)

# nros_second_staticlib_weaken_command(<out_var> ARCHIVE <path> KEEP <glob>
#                                      [OBJCOPY <tool>])
#
# Sets <out_var> to the COMMAND list (tool + arguments) that weakens every
# global of <path> not matching <glob>, in place. OBJCOPY defaults to
# CMAKE_OBJCOPY, which every Zephyr toolchain defines.
function(nros_second_staticlib_weaken_command _out)
    cmake_parse_arguments(_W "" "ARCHIVE;KEEP;OBJCOPY" "" ${ARGN})
    if(NOT _W_ARCHIVE OR NOT _W_KEEP)
        message(FATAL_ERROR
            "nros_second_staticlib_weaken_command: ARCHIVE and KEEP are required")
    endif()
    set(_tool "${_W_OBJCOPY}")
    if(NOT _tool)
        set(_tool "${CMAKE_OBJCOPY}")
    endif()
    if(NOT _tool)
        message(FATAL_ERROR
            "nros: no objcopy to weaken ${_W_ARCHIVE} with (CMAKE_OBJCOPY is "
            "unset). A second Rust staticlib in one image must not define the "
            "runtime's compiler-rt intrinsics strongly, or the link fails with "
            "`multiple definition of __popcountdi2` and its siblings (issue 2092).")
    endif()
    # `!<glob>` first: with --wildcard, a symbol matching a `!` pattern is
    # excluded from the option, and `*` then takes every other global.
    set(${_out}
        "${_tool}" --wildcard
        "--weaken-symbol=!${_W_KEEP}"
        "--weaken-symbol=*"
        "${_W_ARCHIVE}"
        PARENT_SCOPE)
endfunction()
