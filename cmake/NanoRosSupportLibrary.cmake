# cmake/NanoRosSupportLibrary.cmake — phase-383 W7.e / W7.f (RFC-0065 D12)
#
# `nano_ros_support_library()` — third-party code that has NO REFERENCING SYMBOL
# enters the image through an ordinary package, and force-linking is a KEYWORD
# this file implements rather than a flag the user writes.
#
# ===========================================================================
# WHY THIS FILE EXISTS
# ===========================================================================
#
# RFC-0065 D12 splits third-party code into three shapes:
#
#   * an app-level library a node calls  -> a node package dependency (works today)
#   * an SoC/board vendor layer          -> a board crate, RFC-0012 / D11 (works today)
#   * code with no referencing symbol    -> THIS FILE
#
# The third shape is vector tables, driver/init tables collected by the linker,
# a vendor MCAL layer whose registration objects nothing calls, or a prebuilt
# `.a` that must be linked whole. Dead-code elimination (`--gc-sections`, and
# ld's own "only pull an archive member that resolves an undefined symbol"
# rule) drops all of it. Under D4 the entry package is GENERATED, so there is
# no entry `CMakeLists.txt` for a user to add a link line to either.
#
# ===========================================================================
# WHY A KEYWORD AND NOT A FLAG THE USER WRITES — issue 0475
# ===========================================================================
#
# We already whole-archive the RMW backend through a raw
# `-Wl,--whole-archive,$<TARGET_FILE:...>,--no-whole-archive` (root
# `CMakeLists.txt`), and issue 0475 records exactly what that construct costs:
#
#   CMake cannot see a file inside a flag string.
#
# So the flag carries NO REBUILD EDGE. `add_dependencies()` supplies only build
# ORDER, which ninja renders as an order-only (`||`) edge — "must exist before
# linking", never "relink when it changes". Measured on
# `examples/native/c/talker/build-cyclonedds`: the backend archive rebuilt at
# 14:15 while `c_talker` stayed at 12:28 and `cmake --build` exited 0 doing
# nothing. The executable kept the OLD backend indefinitely — museum binaries
# by construction — and only `rm -rf` on the build dir cleared it (~687 s per
# leaf).
#
# Asking a user to hand-write the flag is asking them to reproduce a defect we
# have already paid for. Behind the keyword we emit the flag AND the
# `LINK_DEPENDS` that gives it a file-level edge on the consuming target's link
# rule.
#
# Verify on any consumer:
#     ninja -C <build-dir> -t query <exe>
# the support archive must appear under `|` (implicit), never only under `||`.
#
# ===========================================================================
# WHY OWNING THE SPELLING IS THE POINT — the 3.22 floor
# ===========================================================================
#
# CMake 3.24 has `$<LINK_LIBRARY:WHOLE_ARCHIVE,tgt>`, and CMake's own docs say
# projects should prefer it "instead of manual implementations". Our floor is
# **3.22** (`cmake_minimum_required` in the repo root), so it is out of reach.
#
# ==> When the floor rises to 3.24, `_nano_ros_support_whole_archive_link()`
#     below is THE ONLY FUNCTION THAT CHANGES. No user file moves, because no
#     user file ever spelled the flag. That property is the whole reason D12
#     makes force-linking a keyword instead of documentation.
#
# ESP-IDF reached the same conclusion independently:
# `idf_component_register(SRCS … WHOLE_ARCHIVE)` is a declared keyword on the
# component, not a flag its author writes.
#
# ===========================================================================
# SIGNATURE
# ===========================================================================
#
#   nano_ros_support_library(<name>
#       [SRCS <file>...]              # sources compiled into a STATIC library;
#                                     #   globs allowed (CONFIGURE_DEPENDS)
#       [ARCHIVE <path-to.a>]         # a PREBUILT archive instead of SRCS
#       [INCLUDES <dir>...]           # PUBLIC include dirs
#       [DEFINES <def>...]            # PUBLIC compile definitions
#       [WHOLE_ARCHIVE]               # force-link every member (+ LINK_DEPENDS)
#       [LINKER_FRAGMENTS <f.ld>...]  # W7.f — `.ld` snippets for the image link
#       [ZEPHYR_SECTION <location>])  # W7.f — zephyr_linker_sources() location,
#                                     #   default SECTIONS
#
#   nano_ros_link_support_libraries(<target> [LIBRARIES <name>...])
#
#       Declares `<target>` a consumer. With no LIBRARIES, every support
#       library declared ANYWHERE in this configure is attached.
#
# The attach is DEFERRED to the end of the top-level directory scope
# (`cmake_language(DEFER DIRECTORY ${CMAKE_SOURCE_DIR} …)`), which is the same
# ordering problem `nros_platform_link_app_deferred()` in `NanoRosEntry.cmake`
# solves one layer down. It means the two calls may appear in EITHER order and
# in ANY directory: a support package deep in `src/` can be declared long after
# the image executable was created, or long before it exists.

include_guard(GLOBAL)

# ---------------------------------------------------------------------------
# _nano_ros_support_whole_archive_link(<out_var> <file-genex>)
#
# THE ONE PLACE THE FORCE-LINK SPELLING LIVES. See the 3.22-floor note above:
# raising the floor to 3.24 replaces this body with `$<LINK_LIBRARY:...>` and
# nothing else in the tree (and nothing at all in user packages) moves.
#
# The GNU form matches the root `CMakeLists.txt` byte for byte, deliberately:
# `--whole-archive` is a MODE, not a per-file flag, so it stays on for every
# library ld sees afterwards on its single pass. The closing `--no-whole-archive`
# is what keeps it scoped to this one archive.
# ---------------------------------------------------------------------------
function(_nano_ros_support_whole_archive_link out_var file_expr)
    if(MSVC)
        set(${out_var} "/WHOLEARCHIVE:${file_expr}" PARENT_SCOPE)
    elseif(APPLE)
        # ld64 has no --whole-archive; -force_load takes the archive directly.
        set(${out_var} "-Wl,-force_load,${file_expr}" PARENT_SCOPE)
    else()
        set(${out_var}
            "-Wl,--whole-archive,${file_expr},--no-whole-archive" PARENT_SCOPE)
    endif()
endfunction()

# ---------------------------------------------------------------------------
# _nano_ros_support_expand_srcs(<out_var> <entry>...)
#
# D12's own example writes `SRCS generated/*.c` — a vendor tool emits a
# directory of C and the user does not want to enumerate it. `add_library()`
# does not glob, so we do, and CONFIGURE_DEPENDS is MANDATORY here for the same
# reason `check-interface-glob-configure-depends` makes it mandatory over
# `msg/*.msg`: `file(GLOB)` captures the set at CONFIGURE time, so a file added
# afterwards is silently absent until something unrelated forces a reconfigure.
# For a support library that means a vendor init object quietly missing from
# the image — the failure mode this whole file exists to prevent.
#
# An empty glob is a FATAL_ERROR, not an empty list: "I linked nothing" must
# not be indistinguishable from "there was nothing to link".
# ---------------------------------------------------------------------------
function(_nano_ros_support_expand_srcs out_var)
    set(_out "")
    foreach(_entry IN LISTS ARGN)
        if(_entry MATCHES "[*?]")
            if(IS_ABSOLUTE "${_entry}")
                set(_pattern "${_entry}")
            else()
                set(_pattern "${CMAKE_CURRENT_SOURCE_DIR}/${_entry}")
            endif()
            file(GLOB _matched CONFIGURE_DEPENDS "${_pattern}")
            if(NOT _matched)
                message(FATAL_ERROR
                    "nano_ros_support_library: SRCS pattern '${_entry}' matched "
                    "no files (looked in '${CMAKE_CURRENT_SOURCE_DIR}').\n"
                    "  * If the vendor tool has not run yet, run it before "
                    "`nros build` — RFC-0065 D12: a vendor tool's output is "
                    "COMMITTED, never invoked by us.\n"
                    "  * If the pattern is simply wrong, spell the files out; "
                    "SRCS accepts plain paths too.")
            endif()
            list(APPEND _out ${_matched})
        else()
            list(APPEND _out "${_entry}")
        endif()
    endforeach()
    set(${out_var} "${_out}" PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_support_library(<name> …)
# ---------------------------------------------------------------------------
function(nano_ros_support_library _NRSL_NAME)
    cmake_parse_arguments(_NRSL
        "WHOLE_ARCHIVE"
        "ARCHIVE;ZEPHYR_SECTION"
        "SRCS;INCLUDES;DEFINES;LINKER_FRAGMENTS"
        ${ARGN})

    if(_NRSL_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nano_ros_support_library(${_NRSL_NAME}): unexpected argument(s): "
            "${_NRSL_UNPARSED_ARGUMENTS}\n"
            "Keywords are SRCS, ARCHIVE, INCLUDES, DEFINES, WHOLE_ARCHIVE, "
            "LINKER_FRAGMENTS, ZEPHYR_SECTION.")
    endif()
    if("${_NRSL_NAME}" STREQUAL "")
        message(FATAL_ERROR
            "nano_ros_support_library: <name> is required and must come first, "
            "e.g. nano_ros_support_library(rtd_mcal SRCS generated/*.c "
            "INCLUDES include WHOLE_ARCHIVE).")
    endif()
    if(TARGET ${_NRSL_NAME})
        message(FATAL_ERROR
            "nano_ros_support_library(${_NRSL_NAME}): a target named "
            "'${_NRSL_NAME}' already exists. A support library IS a cmake "
            "target, so the name must be free — rename the support library, "
            "or drop the duplicate declaration.")
    endif()

    # SRCS and ARCHIVE are mutually exclusive ON PURPOSE. Both would have to
    # land in one archive for WHOLE_ARCHIVE to mean one thing, and CMake cannot
    # merge a prebuilt `.a` into a STATIC target it also compiles. Two
    # declarations say what one ambiguous declaration cannot.
    if(_NRSL_SRCS AND _NRSL_ARCHIVE)
        message(FATAL_ERROR
            "nano_ros_support_library(${_NRSL_NAME}): SRCS and ARCHIVE are "
            "mutually exclusive — a prebuilt archive cannot be merged with "
            "compiled sources into one library. Declare TWO support "
            "libraries (e.g. '${_NRSL_NAME}' with ARCHIVE and "
            "'${_NRSL_NAME}_glue' with SRCS); both are attached to the image.")
    endif()
    if(NOT _NRSL_SRCS AND NOT _NRSL_ARCHIVE AND NOT _NRSL_LINKER_FRAGMENTS)
        message(FATAL_ERROR
            "nano_ros_support_library(${_NRSL_NAME}): nothing to contribute — "
            "pass SRCS, ARCHIVE, or LINKER_FRAGMENTS. A support package that "
            "carries only headers is an ordinary INTERFACE library; it does "
            "not need this function.")
    endif()
    if(_NRSL_WHOLE_ARCHIVE AND NOT _NRSL_SRCS AND NOT _NRSL_ARCHIVE)
        message(FATAL_ERROR
            "nano_ros_support_library(${_NRSL_NAME}): WHOLE_ARCHIVE without "
            "SRCS or ARCHIVE — there is no archive to force-link. Drop the "
            "keyword, or add the code it was meant to keep.")
    endif()

    # -- the target -----------------------------------------------------------
    if(_NRSL_ARCHIVE)
        if(IS_ABSOLUTE "${_NRSL_ARCHIVE}")
            set(_archive "${_NRSL_ARCHIVE}")
        else()
            set(_archive "${CMAKE_CURRENT_SOURCE_DIR}/${_NRSL_ARCHIVE}")
        endif()
        # Checked at CONFIGURE time and fatal: a prebuilt archive is COMMITTED
        # (D12 — "a vendor tool's output is committed, never invoked by us"),
        # so a missing one is a declaration error, and a declaration error must
        # fail in stages 1-2 naming the file, never mid-compile as an
        # unresolved-symbol wall three minutes later.
        if(NOT EXISTS "${_archive}")
            message(FATAL_ERROR
                "nano_ros_support_library(${_NRSL_NAME}): ARCHIVE "
                "'${_archive}' does not exist.\n"
                "A prebuilt archive is committed with the support package. If "
                "this archive is BUILT by the workspace, do not name it here — "
                "declare its sources with SRCS instead, so cmake owns the "
                "rebuild edge.")
        endif()
        add_library(${_NRSL_NAME} STATIC IMPORTED GLOBAL)
        set_target_properties(${_NRSL_NAME} PROPERTIES
            IMPORTED_LOCATION "${_archive}")
        if(_NRSL_INCLUDES)
            set_property(TARGET ${_NRSL_NAME} APPEND PROPERTY
                INTERFACE_INCLUDE_DIRECTORIES ${_NRSL_INCLUDES})
        endif()
        if(_NRSL_DEFINES)
            set_property(TARGET ${_NRSL_NAME} APPEND PROPERTY
                INTERFACE_COMPILE_DEFINITIONS ${_NRSL_DEFINES})
        endif()
    elseif(_NRSL_SRCS)
        _nano_ros_support_expand_srcs(_srcs ${_NRSL_SRCS})
        add_library(${_NRSL_NAME} STATIC ${_srcs})
        if(_NRSL_INCLUDES)
            target_include_directories(${_NRSL_NAME} PUBLIC ${_NRSL_INCLUDES})
        endif()
        if(_NRSL_DEFINES)
            target_compile_definitions(${_NRSL_NAME} PUBLIC ${_NRSL_DEFINES})
        endif()
    else()
        # LINKER_FRAGMENTS-only: still a target, so a consumer can name it in
        # `nano_ros_link_support_libraries(<t> LIBRARIES …)` and so the
        # bookkeeping below has exactly one shape.
        add_library(${_NRSL_NAME} INTERFACE)
        if(_NRSL_INCLUDES)
            target_include_directories(${_NRSL_NAME} INTERFACE ${_NRSL_INCLUDES})
        endif()
        if(_NRSL_DEFINES)
            target_compile_definitions(${_NRSL_NAME} INTERFACE ${_NRSL_DEFINES})
        endif()
    endif()

    # -- bookkeeping ----------------------------------------------------------
    #
    # Per-library facts live in GLOBAL properties keyed by name rather than as
    # custom target properties: an IMPORTED / INTERFACE target restricts which
    # properties may be set on it, and the ARCHIVE arm produces exactly those.
    # One storage shape for all three arms beats two that diverge.
    set_property(GLOBAL PROPERTY
        NROS_SUPPORT_LIB_${_NRSL_NAME}_WHOLE_ARCHIVE "${_NRSL_WHOLE_ARCHIVE}")

    if(_NRSL_LINKER_FRAGMENTS)
        _nano_ros_support_declare_fragments("${_NRSL_NAME}"
            "${_NRSL_ZEPHYR_SECTION}" ${_NRSL_LINKER_FRAGMENTS})
    endif()

    set_property(GLOBAL APPEND PROPERTY NROS_SUPPORT_LIBRARIES "${_NRSL_NAME}")
    _nano_ros_support_schedule_flush()
endfunction()

# ---------------------------------------------------------------------------
# _nano_ros_support_declare_fragments(<lib> <zephyr_section> <file>...)
#
# W7.f — RFC-0065 D12 covered libraries but not `.ld` fragments, and the
# motivating downstream (`autoware-safety-island`) carries four:
# `discard_unwind.ld`, `node_stack_in_sram.ld`, `netc_bd_no_cacheable.ld`,
# `heap_in_sram.ld`. The NETC buffer descriptors MUST land in a non-cacheable
# region — that is a memory-map fact, not a preference, so the fragment has to
# reach the link or the board mis-DMAs.
#
# TWO SEAMS, because there are two link models in the tree:
#
#   * ZEPHYR — `zephyr_linker_sources(<location> <file>)` appends an `#include`
#     to a snippet file that `linker.ld` pulls in. Zephyr does the including;
#     we only hand it the file. Called HERE (declaration time) because the
#     snippet is global to the Zephyr build and does not need a consumer.
#
#   * EVERYONE ELSE (FreeRTOS/ThreadX/NuttX bare-metal GCC) — the board owns a
#     top-level `.ld` that `INCLUDE`s fragments by BARE NAME, and `INCLUDE`
#     resolves against the linker SEARCH PATH. `nano-ros-board-mps2-an385-
#     freertos.cmake` already establishes this idiom
#     (`-L${_NROS_FREERTOS_SHARED_CONFIG_DIR}`, phase-337 W5.e). So we put each
#     fragment's directory on the consumer's `-L` path at attach time, and add
#     the fragment file to `LINK_DEPENDS`.
#
# The `LINK_DEPENDS` half is the same issue-0475 lesson as WHOLE_ARCHIVE: a
# linker script reached through a flag string is invisible to cmake, so editing
# a fragment would otherwise NOT relink. Both seams get it.
# ---------------------------------------------------------------------------
function(_nano_ros_support_declare_fragments lib zephyr_section)
    set(_abs "")
    foreach(_frag IN LISTS ARGN)
        if(IS_ABSOLUTE "${_frag}")
            set(_p "${_frag}")
        else()
            set(_p "${CMAKE_CURRENT_SOURCE_DIR}/${_frag}")
        endif()
        if(NOT EXISTS "${_p}")
            message(FATAL_ERROR
                "nano_ros_support_library(${lib}): LINKER_FRAGMENTS file "
                "'${_p}' does not exist. Fragments are authored files in the "
                "support package; a generated one must be produced before "
                "configure.")
        endif()
        if(IS_DIRECTORY "${_p}")
            message(FATAL_ERROR
                "nano_ros_support_library(${lib}): LINKER_FRAGMENTS entry "
                "'${_p}' is a directory. Name each `.ld` file.")
        endif()
        list(APPEND _abs "${_p}")
    endforeach()

    set_property(GLOBAL PROPERTY
        NROS_SUPPORT_LIB_${lib}_FRAGMENTS "${_abs}")

    if(COMMAND zephyr_linker_sources)
        # Zephyr's own vocabulary: SECTIONS / ROM_START / RAM_SECTIONS /
        # DATA_SECTIONS / NOCACHE_SECTION / … `SECTIONS` is the general slot and
        # the right default; a fragment that must land somewhere specific (the
        # NETC no-cacheable case) names it with ZEPHYR_SECTION.
        set(_loc "${zephyr_section}")
        if("${_loc}" STREQUAL "")
            set(_loc "SECTIONS")
        endif()
        foreach(_p IN LISTS _abs)
            zephyr_linker_sources(${_loc} "${_p}")
        endforeach()
        set_property(GLOBAL PROPERTY
            NROS_SUPPORT_LIB_${lib}_FRAGMENTS_ZEPHYR TRUE)
    elseif(zephyr_section)
        message(FATAL_ERROR
            "nano_ros_support_library(${lib}): ZEPHYR_SECTION "
            "'${zephyr_section}' was passed but this is not a Zephyr build "
            "(`zephyr_linker_sources` is not defined). On other platforms the "
            "board's top-level linker script INCLUDEs the fragment by name and "
            "chooses its own placement — drop ZEPHYR_SECTION.")
    endif()
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_link_support_libraries(<target> [LIBRARIES <name>...])
# ---------------------------------------------------------------------------
function(nano_ros_link_support_libraries _NRLS_TARGET)
    cmake_parse_arguments(_NRLS "" "" "LIBRARIES" ${ARGN})
    if(_NRLS_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nano_ros_link_support_libraries(${_NRLS_TARGET}): unexpected "
            "argument(s): ${_NRLS_UNPARSED_ARGUMENTS} — the only keyword is "
            "LIBRARIES.")
    endif()
    if(NOT TARGET ${_NRLS_TARGET})
        message(FATAL_ERROR
            "nano_ros_link_support_libraries: '${_NRLS_TARGET}' is not a "
            "target. Call this AFTER the executable/library exists; the "
            "support libraries themselves may be declared before or after, "
            "since the attach is deferred to the end of the top-level scope.")
    endif()
    set_property(GLOBAL APPEND PROPERTY
        NROS_SUPPORT_CONSUMERS "${_NRLS_TARGET}")
    set_property(GLOBAL PROPERTY
        NROS_SUPPORT_CONSUMER_${_NRLS_TARGET}_LIBRARIES "${_NRLS_LIBRARIES}")
    _nano_ros_support_schedule_flush()
endfunction()

# ---------------------------------------------------------------------------
# _nano_ros_support_schedule_flush()
#
# Deferred attach, the `nros_platform_link_app_deferred()` pattern one layer up.
# `cmake_language(DEFER DIRECTORY <top-level>)` runs the call after every
# `add_subdirectory()` has been processed, so BOTH orders work:
#
#   declare support lib -> create image -> link      (support pkg first)
#   create image -> link -> declare support lib      (image pkg first)
#
# Neither order can be forbidden: under D4 the entry package is generated and
# the builder decides where it lands in the subdirectory order, and a hostile
# tree (W8) puts a support package wherever it already was.
#
# The DEFER target is the TOP-LEVEL scope specifically. Deferring to the
# CURRENT directory would fire at the end of whichever package happened to call
# first, i.e. before the other package was even read — which is the bug this
# indirection exists to avoid, not a smaller version of it.
# ---------------------------------------------------------------------------
function(_nano_ros_support_schedule_flush)
    get_property(_scheduled GLOBAL PROPERTY NROS_SUPPORT_FLUSH_SCHEDULED)
    if(_scheduled)
        return()
    endif()
    set_property(GLOBAL PROPERTY NROS_SUPPORT_FLUSH_SCHEDULED TRUE)
    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
        CALL _nano_ros_support_flush)
endfunction()

function(_nano_ros_support_flush)
    get_property(_libs GLOBAL PROPERTY NROS_SUPPORT_LIBRARIES)
    get_property(_consumers GLOBAL PROPERTY NROS_SUPPORT_CONSUMERS)
    if(NOT _libs OR NOT _consumers)
        # One half with no other half is legitimate: a workspace may declare a
        # support package for an image that this configure does not build, and
        # every ordinary image calls the link verb whether or not any support
        # package exists.
        return()
    endif()
    list(REMOVE_DUPLICATES _consumers)
    foreach(_consumer IN LISTS _consumers)
        get_property(_selected GLOBAL PROPERTY
            NROS_SUPPORT_CONSUMER_${_consumer}_LIBRARIES)
        if(_selected)
            foreach(_lib IN LISTS _selected)
                if(NOT TARGET ${_lib})
                    message(FATAL_ERROR
                        "nano_ros_link_support_libraries(${_consumer}): "
                        "LIBRARIES named '${_lib}', but no support library by "
                        "that name was declared in this configure. Declared: "
                        "${_libs}")
                endif()
            endforeach()
            set(_attach "${_selected}")
        else()
            set(_attach "${_libs}")
        endif()
        foreach(_lib IN LISTS _attach)
            _nano_ros_support_attach("${_lib}" "${_consumer}")
        endforeach()
    endforeach()
endfunction()

# ---------------------------------------------------------------------------
# _nano_ros_support_attach(<lib> <consumer>)
# ---------------------------------------------------------------------------
function(_nano_ros_support_attach lib consumer)
    # Idempotent: `nano_ros_link_support_libraries` may be reached twice for one
    # target (a verb calls it and the user calls it too), and a duplicated
    # `--whole-archive` group is not harmless — ld sees the archive twice on its
    # single pass and duplicate-symbol errors follow.
    get_property(_done GLOBAL PROPERTY NROS_SUPPORT_ATTACHED_${consumer})
    if("${lib}" IN_LIST _done)
        return()
    endif()
    set_property(GLOBAL APPEND PROPERTY
        NROS_SUPPORT_ATTACHED_${consumer} "${lib}")

    get_property(_whole GLOBAL PROPERTY NROS_SUPPORT_LIB_${lib}_WHOLE_ARCHIVE)
    get_property(_frags GLOBAL PROPERTY NROS_SUPPORT_LIB_${lib}_FRAGMENTS)
    get_target_property(_type ${lib} TYPE)

    if(_whole AND NOT _type STREQUAL "INTERFACE_LIBRARY")
        # Usage requirements are propagated by hand rather than by
        # `target_link_libraries()`. Issue 0475's second half: putting the
        # archive on the link line as a library IN ADDITION to naming it inside
        # the `--whole-archive` flag reorders ld's single pass and breaks the
        # group (the RMW case failed with `undefined reference to ddsrt_*`).
        # A genex read of the INTERFACE properties gives the headers and
        # defines without touching the link line at all.
        target_include_directories(${consumer} PRIVATE
            "$<TARGET_PROPERTY:${lib},INTERFACE_INCLUDE_DIRECTORIES>")
        target_compile_definitions(${consumer} PRIVATE
            "$<TARGET_PROPERTY:${lib},INTERFACE_COMPILE_DEFINITIONS>")

        _nano_ros_support_whole_archive_link(_flag "$<TARGET_FILE:${lib}>")
        target_link_options(${consumer} PRIVATE "${_flag}")

        # THE EDGE THE FLAG CANNOT CARRY (issue 0475). LINK_DEPENDS attaches to
        # THIS target's link rule, so a changed archive relinks the consumer.
        set_property(TARGET ${consumer} APPEND PROPERTY
            LINK_DEPENDS "$<TARGET_FILE:${lib}>")

        # ORDER, separately: the flag also gives ninja no reason to BUILD the
        # archive before linking. `add_dependencies` is exactly the order-only
        # edge 0475 says is insufficient on its own — paired with LINK_DEPENDS
        # above it is the missing half, not a substitute.
        get_target_property(_imported ${lib} IMPORTED)
        if(NOT _imported)
            add_dependencies(${consumer} ${lib})
        endif()
    else()
        # No force-link asked for (or nothing to force-link): an ordinary link,
        # where cmake already owns both the rebuild edge and the order.
        target_link_libraries(${consumer} PRIVATE ${lib})
    endif()

    if(_frags)
        get_property(_zephyr GLOBAL PROPERTY
            NROS_SUPPORT_LIB_${lib}_FRAGMENTS_ZEPHYR)
        set(_dirs "")
        foreach(_frag IN LISTS _frags)
            # Relink when a fragment is edited — on BOTH seams. Zephyr includes
            # the snippet into `linker.ld`; the bare-metal boards `INCLUDE` it
            # from the board script. Neither path is visible to cmake as a
            # dependency, which is issue 0475's class with a `.ld` instead of a
            # `.a`.
            set_property(TARGET ${consumer} APPEND PROPERTY
                LINK_DEPENDS "${_frag}")
            get_filename_component(_d "${_frag}" DIRECTORY)
            list(APPEND _dirs "${_d}")
        endforeach()
        if(NOT _zephyr)
            if(MSVC)
                message(FATAL_ERROR
                    "nano_ros_support_library(${lib}): LINKER_FRAGMENTS is a "
                    "GNU-ld / Zephyr concept; MSVC has no `INCLUDE` directive "
                    "for linker scripts.")
            endif()
            list(REMOVE_DUPLICATES _dirs)
            foreach(_d IN LISTS _dirs)
                # `INCLUDE <name>.ld` inside the board's top-level script
                # resolves against the linker search path — the same mechanism
                # `-L${_NROS_FREERTOS_SHARED_CONFIG_DIR}` uses today.
                target_link_options(${consumer} PRIVATE "-L${_d}")
            endforeach()
        endif()
    endif()
endfunction()
