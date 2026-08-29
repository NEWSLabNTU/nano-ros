# RFC-0085 D14 — make a west build and its `[image.*]` provably agree.
#
# WHY A CROSS-CHECK AND NOT A REPLACEMENT
#
# D14 was written as "a west build derives its cargo invocation from `[image.*]`
# instead of re-spelling it from Kconfig". Implementing that literally would
# break the promise the whole RFC is built on: a plain Zephyr application using
# nano-ros as a module is NOT in a nano-ros workspace, has no image, and must
# keep working with `west build -b <board> <app>`. If Kconfig stopped being the
# mechanism, that app would have nothing left to derive from.
#
# So Kconfig stays the mechanism, and the image becomes the CHECK:
#
#   * no workspace  -> nothing to check, build proceeds (the standalone app)
#   * workspace     -> the two answers must agree, or the build stops
#
# That gets D14's actual goal — the two derivations cannot silently disagree —
# without taking the standalone path away. The disagreement is not theoretical:
# `[image.demo] rmw = "cyclonedds"` with a `prj-zenoh.conf` on the `conf` list
# builds a zenoh image while the workspace says Cyclone, and every symptom of
# that appears at RUNTIME as "nothing is discovered", one layer below anything
# that names an RMW.
#
# WHY HERE
#
# `zephyr/CMakeLists.txt` is the module: it runs for every nano-ros Zephyr
# build, workspace or not, which is exactly the set that needs the check.
#
# Modelled on `NanoRosBoardFacts.cmake` — same CLI resolution, same soft
# degradation when there is no CLI. A missing `nros` must never fail a build
# that was not asking it anything.
include_guard(GLOBAL)

# nros_check_image_agreement(KCONFIG_RMW <rmw>)
#
# `<rmw>` is what Kconfig selected, lowercase (`zenoh` / `xrce` / `cyclonedds`),
# or empty when no backend is configured — which is legal for a build that only
# compiles the library.
function(nros_check_image_agreement)
    cmake_parse_arguments(_A "" "KCONFIG_RMW" "" ${ARGN})

    # The CLI, under whichever name this lane resolved it. The Zephyr lane uses
    # `_NROS_ZEPHYR_CODEGEN_TOOL` (`nros_generate_interfaces.cmake`), the rest
    # use `_NANO_ROS_CODEGEN_TOOL` — and a check that knew only one of them
    # would silently do nothing on the lane it was written for.
    set(_nros "")
    foreach(_cand "${_NROS_ZEPHYR_CODEGEN_TOOL}" "${_NANO_ROS_CODEGEN_TOOL}")
        if(_nros STREQUAL "" AND NOT _cand STREQUAL "" AND EXISTS "${_cand}")
            set(_nros "${_cand}")
        endif()
    endforeach()
    # Neither cache var is guaranteed to be populated where this runs — the
    # Zephyr lane fills `_NROS_ZEPHYR_CODEGEN_TOOL` only if
    # `nros_generate_interfaces.cmake` was reached, which depends on the app.
    # Measured: a build printed `board facts NOT delivered — no nros CLI` and
    # this check went silent for the same reason, having checked nothing.
    #
    # `nros_resolve_cli(… OPTIONAL)` is the primitive both of those go through
    # and the only one that answers without failing when there is no CLI.
    if(_nros STREQUAL "" AND COMMAND nros_resolve_cli)
        nros_resolve_cli(_found OPTIONAL)
        if(_found AND EXISTS "${_found}")
            set(_nros "${_found}")
        endif()
    endif()
    if(_nros STREQUAL "")
        # No CLI is not an error here: this function only cross-checks, so its
        # absence costs a check rather than a build.
        return()
    endif()
    if(NOT APPLICATION_SOURCE_DIR)
        return()
    endif()

    get_filename_component(_entry_pkg "${APPLICATION_SOURCE_DIR}" NAME)

    # `--if-present` makes "this app is not in a workspace" an empty answer
    # rather than a failure — the standalone case, and the common one.
    execute_process(
        COMMAND "${_nros}" image-facts
                --for-entry "${_entry_pkg}"
                --workspace "${APPLICATION_SOURCE_DIR}"
                --cmake --if-present
        OUTPUT_VARIABLE _facts
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)

    if(NOT _rc EQUAL 0)
        # The verb failed for a reason `--if-present` does not cover — a
        # malformed `system.toml`, say. Report it and carry on: a build that
        # was succeeding before this file existed must not start failing
        # because a CHECK could not run.
        message(STATUS "nano-ros: image agreement NOT checked — ${_err}")
        return()
    endif()
    if(_facts STREQUAL "")
        # Not in a nano-ros workspace. Nothing declared the image, so there is
        # no second answer to disagree with.
        return()
    endif()

    # The facts are `set()` lines, so evaluating them IS the parse. Written to
    # a file and included rather than `cmake_language(EVAL)` so a syntax error
    # points at a line someone can read.
    set(_facts_file "${CMAKE_CURRENT_BINARY_DIR}/nros-image-facts.cmake")
    file(WRITE "${_facts_file}" "${_facts}\n")
    include("${_facts_file}")

    if(NOT DEFINED NROS_IMAGE_RMW OR NROS_IMAGE_RMW STREQUAL "")
        # The image names no RMW, so it defers to Kconfig. Legal, and the
        # common case for an image that only pins a board.
        set(_declared "")
    else()
        string(TOLOWER "${NROS_IMAGE_RMW}" _declared)
    endif()
    string(TOLOWER "${_A_KCONFIG_RMW}" _selected)

    message(STATUS
        "nano-ros: image ${NROS_IMAGE_QUALIFIED} (rmw=${_declared}) "
        "<- ${NROS_IMAGE_WORKSPACE}")

    if(_declared STREQUAL "" OR _selected STREQUAL "")
        return()
    endif()
    if(NOT _declared STREQUAL _selected)
        # ONE string with no blank lines: cmake indents every line of a
        # FATAL_ERROR by two spaces, so an empty one renders as stray
        # whitespace rather than a paragraph break.
        message(FATAL_ERROR
"nano-ros: this build and its image disagree about the RMW.
    image ${NROS_IMAGE_QUALIFIED} declares: ${_declared}
    Kconfig selected:                       ${_selected}
Kconfig is what the build uses, so the image's `rmw` is not being honoured.
Either name the matching overlay on the image —
    [image.…]
    conf = [\"prj-${_declared}.conf\"]
— or set `rmw = \"${_selected}\"` if the overlay is the one you want.
Declared in ${NROS_IMAGE_WORKSPACE}.")
    endif()
endfunction()
