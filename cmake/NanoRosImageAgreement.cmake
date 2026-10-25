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
# `[image.demo] rmw = "cyclonedds"` with a `prj-zenoh.conf` on the `os_config` list
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

# _nros_image_facts_file(<out-var>)
#
# Run `nros image-facts --for-entry <app> --cmake --if-present` ONCE per
# configure and set <out-var> to the file holding its `set(NROS_IMAGE_* …)`
# lines, or to "" when there is no answer (no CLI, no application, not in a
# workspace, or the query failed). Two readers ask (the capability axes early,
# the RMW cross-check late), and a query plans the whole workspace, so the
# second asks the first's answer rather than planning again.
function(_nros_image_facts_file out_var)
    get_property(_asked GLOBAL PROPERTY NROS_IMAGE_FACTS_ASKED)
    if(_asked)
        get_property(_f GLOBAL PROPERTY NROS_IMAGE_FACTS_FILE)
        set(${out_var} "${_f}" PARENT_SCOPE)
        return()
    endif()
    set(${out_var} "" PARENT_SCOPE)

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

    # Remembered only from here on: an early caller that found no CLI must not
    # stop a later one, after a lane has resolved it, from asking.
    set_property(GLOBAL PROPERTY NROS_IMAGE_FACTS_ASKED TRUE)
    set_property(GLOBAL PROPERTY NROS_IMAGE_FACTS_FILE "")

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
        message(STATUS "nano-ros: image facts NOT read (agreement and capability axes unchecked) — ${_err}")
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
    set_property(GLOBAL PROPERTY NROS_IMAGE_FACTS_FILE "${_facts_file}")
    set(${out_var} "${_facts_file}" PARENT_SCOPE)
endfunction()

# nros_resolve_image_features()
#
# Issue 1261 -- one source for an image's capability axes on the WEST road.
#
# The module picks nros-c / nros-cpp's cargo features from `NROS_FEATURES`
# during `find_package(Zephyr)`, while codegen reads the bringup's
# `system.toml`. The generated west application sets the variable before
# `find_package` (issue 1681); a HAND-WRITTEN one had to restate the list on
# its `west build` line, and one that did not built nros-cpp without
# `param-services` and failed at LINK on
# `nros_cpp_register_parameter_services`.
#
# So ask the image (same query, same SSoT accessor as `nros config show`):
#
#   * not in a workspace, or no image names this app -> nothing changes
#     (the standalone app keeps `-DNROS_FEATURES` as its only source);
#   * NROS_FEATURES unset -> it becomes the image's declared axes;
#   * NROS_FEATURES set and missing a declared axis -> refused HERE,
#     naming the axes, rather than at link as an undefined symbol.
#
# A superset is allowed: building a capability nobody calls costs size, not
# correctness, and an explicit choice to carry one is not a disagreement.
#
# A MACRO so the default lands in the module's scope, where the readers are.
macro(nros_resolve_image_features)
    _nros_image_facts_file(_nrif_file)
    if(NOT _nrif_file STREQUAL "")
        unset(NROS_IMAGE_FEATURES)
        include("${_nrif_file}")
    endif()
    if(NOT _nrif_file STREQUAL "" AND DEFINED NROS_IMAGE_FEATURES)
        if(NOT DEFINED NROS_FEATURES)
            set(NROS_FEATURES "${NROS_IMAGE_FEATURES}")
            if(NROS_IMAGE_FEATURES STREQUAL "")
                set(_nrif_shown "none")
            else()
                string(REPLACE ";" ", " _nrif_shown "${NROS_IMAGE_FEATURES}")
            endif()
            message(STATUS
                "nano-ros: capability axes of image ${NROS_IMAGE_QUALIFIED}: "
                "${_nrif_shown} (from its system.toml)")
        else()
            set(_nrif_missing "")
            foreach(_nrif_axis IN LISTS NROS_IMAGE_FEATURES)
                if(NOT _nrif_axis IN_LIST NROS_FEATURES)
                    list(APPEND _nrif_missing "${_nrif_axis}")
                endif()
            endforeach()
            if(_nrif_missing)
                string(REPLACE ";" ", " _nrif_missing "${_nrif_missing}")
                message(FATAL_ERROR
"nano-ros: NROS_FEATURES omits capability axes the image declares.
    image ${NROS_IMAGE_QUALIFIED} declares: ${NROS_IMAGE_FEATURES}
    NROS_FEATURES:                     ${NROS_FEATURES}
    missing:                               ${_nrif_missing}
The generated entry calls these capabilities and the library would be built
without them, which fails at link with an undefined symbol. Drop
-DNROS_FEATURES (the image's system.toml is the source) or add the missing
axes to it (issue 1261).
Declared in ${NROS_IMAGE_WORKSPACE}.")
            endif()
        endif()
    endif()
endmacro()

# nros_check_image_agreement(KCONFIG_RMW <rmw>)
#
# `<rmw>` is what Kconfig selected, lowercase (`zenoh` / `xrce` / `cyclonedds`),
# or empty when no backend is configured — which is legal for a build that only
# compiles the library.
function(nros_check_image_agreement)
    cmake_parse_arguments(_A "" "KCONFIG_RMW" "" ${ARGN})

    _nros_image_facts_file(_facts_file)
    if(_facts_file STREQUAL "")
        return()
    endif()
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
    os_config = [\"prj-${_declared}.conf\"]
— or set `rmw = \"${_selected}\"` if the overlay is the one you want.
Declared in ${NROS_IMAGE_WORKSPACE}.")
    endif()
endfunction()

# nros_check_generated_app_current()
#
# Issue 1707 -- refuse a configure that EXECUTED a generated west application
# which has since been rewritten.
#
# `nros build` writes the application (`build/<coord>/<image>_entry/
# CMakeLists.txt`) before it configures, and that file is the only carrier of
# the image's capability Kconfig and `NROS_FEATURES` (issues 1681, 1702).
# But the application is ALSO rewritten by every other `plan_builds` over the
# workspace -- and until issue 1716 that included `nros image-facts` above,
# which runs INSIDE this configure. So a configure that started from a stale
# file (its declaration changed since the last `nros build`) read the old
# values, and the file was brought current while it ran. The query now plans
# with `plan_query` and writes nothing; this check stays, because any other
# planner of the workspace (an `nros build` of a sibling image, in another
# terminal) can still rewrite the file mid-configure. The build system is written after that, so it
# is NEWER than its input and no later `ninja` reconfigures: the image keeps the
# previous declaration's Kconfig and features for good. Measured (ninja 1.10):
# `.config` kept `CONFIG_NROS_CAPABILITY_PARAM_SERVICES=y` through `cmake` +
# `ninja` while the application said `n`.
#
# The generated file sets `NROS_GENERATED_APP_DIGEST` to a fingerprint of its
# own body. The variable is what this configure executed; the line on disk is
# what the file says now. They differ exactly when the file changed after cmake
# read it. Refusing leaves the old build system in place, OLDER than the new
# file, so the next build reconfigures from it -- which is the whole remedy.
#
# Deferred to the end of the TOP-LEVEL directory, so a rewrite anywhere in the
# configure is seen, not only one that happened before this module ran.
function(nros_check_generated_app_current)
    if(NOT DEFINED NROS_GENERATED_APP_DIGEST OR NOT APPLICATION_SOURCE_DIR)
        # Not a generated application (a hand-written or plain Zephyr app).
        return()
    endif()
    set(_app "${APPLICATION_SOURCE_DIR}/CMakeLists.txt")
    if(NOT EXISTS "${_app}")
        return()
    endif()
    file(STRINGS "${_app}" _line
        REGEX "^set\\(NROS_GENERATED_APP_DIGEST [0-9a-f]+\\)$" LIMIT_COUNT 1)
    if(_line STREQUAL "")
        # Materialised and edited by its owner: no generated line to compare.
        return()
    endif()
    string(REGEX REPLACE "^set\\(NROS_GENERATED_APP_DIGEST ([0-9a-f]+)\\)$" "\\1"
        _on_disk "${_line}")
    if(_on_disk STREQUAL NROS_GENERATED_APP_DIGEST)
        return()
    endif()
    message(FATAL_ERROR
"nano-ros: the generated west application was rewritten while this configure ran (issue 1707).
    ${_app}
This configure read the PREVIOUS file, so its Kconfig and NROS_FEATURES describe a declaration that no longer holds. Nothing was generated from it.
Re-run the build: it reconfigures from the current file. `nros build <image>` regenerates the application BEFORE configuring and never meets this.
If it repeats on every build, two different nros CLIs render this file differently -- the one that ran `nros build` and the one this configure resolved.")
endfunction()

# The module schedules the check itself: every Zephyr build that loads it gets
# it, and the TOP-LEVEL directory is still being processed when the module is.
macro(nros_defer_generated_app_check)
    if(DEFINED NROS_GENERATED_APP_DIGEST)
        cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
            CALL nros_check_generated_app_current)
    endif()
endmacro()
