# cmake/NanoRosPackageXml.cmake — RFC-0048 §4 (phase-287 W4): package.xml is the SSoT.
#
# The per-package platform delta lives where ament already expects package
# metadata — `package.xml`'s `<export>`:
#
#   <export>
#     <build_type>ament_cmake</build_type>
#     <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/>
#   </export>
#
# This is what keeps the `CMakeLists.txt` byte-identical across platforms: only
# `package.xml` differs, and only in the one `<nano_ros>` line. `deploy="native"`
# needs no board.
#
# `find_package(nano_ros)` calls `nano_ros_read_package_export()` on the
# consumer's package.xml BEFORE it imports nano-ros, so the deploy/rmw values
# reach `NANO_ROS_PLATFORM` / `NANO_ROS_RMW` in time for the `add_subdirectory`
# body; the verbs read the same tuple for their DEPLOY/BOARD defaults.

include_guard(GLOBAL)

# `deploy` attribute → the `NANO_ROS_PLATFORM` module axis. `native` is the host
# build (it maps to the `posix` platform axis value); the RTOS names map 1:1.
function(_nros_deploy_to_platform deploy out_var)
    if(deploy STREQUAL "native" OR deploy STREQUAL "")
        set(${out_var} "posix" PARENT_SCOPE)
    else()
        set(${out_var} "${deploy}" PARENT_SCOPE)
    endif()
endfunction()

# ---------------------------------------------------------------------------
# nros_read_package_xml_body(<path> <out_var>)
#
# Read a package.xml into <out_var> with XML COMMENTS STRIPPED.
#
# Every package.xml reader in this tree matches regexes against raw file text
# (cmake has no XML parser), and a regex cannot tell an element from an element
# quoted inside a comment. So a package.xml that DOCUMENTS a tag —
#
#   <!-- Provision, not consumption. `<nano_ros rmw="zenoh"/>` in a leaf … -->
#
# — silently declares that tag. Found in phase-348 W1: the first provider
# package.xml written explained the difference between the provision and
# consumption exports in a comment, and `nano_ros_read_package_export()` then
# reported the file as consuming `rmw=zenoh`. The file was correct; the reader
# was.
#
# This is ONE helper rather than a strip at each call site because the tree has
# seven such readers, and a fix applied only where the symptom appeared is how
# this repo's recurring classes got that way. Use it for any regex read of a
# package.xml.
#
# The pattern relies on XML comments being unable to contain `--`, so
# `([^-]|-[^-])*` is an exact match for their body. A naive `<!--.*-->` would be
# greedy and eat every element BETWEEN two comments.
# ---------------------------------------------------------------------------
function(nros_read_package_xml_body path out_var)
    file(READ "${path}" _raw)
    string(REGEX REPLACE "<!--([^-]|-[^-])*-->" "" _raw "${_raw}")
    set(${out_var} "${_raw}" PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# RFC-0087 D2 / phase-420 W2 — the `<build_type>` vocabulary.
#
# A nano-ros-owned package declares `nros_cargo` / `nros_cmake` rather than
# borrowing ament's spelling, so a stock `colcon build` refuses firmware
# instead of trying to install it. The tree is not rewritten until W3, so this
# reader must understand BOTH spellings and resolve them to one build path —
# a reader that learned the new spelling by forgetting the old one would break
# 282 packages on the day it landed.
#
# `_NROS_BUILD_TYPE_MAP` is `<raw>=<canonical>`; `_NROS_BUILD_TYPE_RETIRED`
# lists the spellings with no legitimate use in ANY class. The identical rows
# live in `packages/cli/nros-cli-core/src/build_type.rs`, and
# `scripts/check-build-type-spelling.py` parses both and refuses a
# disagreement. Two readers of one rule, implemented separately, is what put
# the reader bugs in this file's header there; two readers of one TABLE, with a
# gate comparing them, is the shape that does not rot.
#
# CACHE INTERNAL, not a plain `set()`: a module `include()`d from inside a
# function loses its normal variables when the frame pops (the `_NROS_ENTRY_DIR`
# pattern; 287-W6 broke every freertos workspace member exactly this way).
# ---------------------------------------------------------------------------
set(_NROS_BUILD_TYPE_MAP
    "nros_cargo=nros_cargo"
    "nros_cmake=nros_cmake"
    "ament_cargo=nros_cargo"
    "ament_cmake=nros_cmake"
    "cargo=nros_cargo"
    "cmake=nros_cmake"
    "ament_nros=nros_cmake"
    "nros_entry=nros_cargo"
    "nros_bringup=nros_cmake"
    CACHE INTERNAL "RFC-0087 D2: raw <build_type> spelling -> canonical spelling")
set(_NROS_BUILD_TYPE_RETIRED
    "ament_nros"
    "nros_entry"
    "nros_bringup"
    CACHE INTERNAL "RFC-0087 D2: <build_type> spellings that no class may keep")

# ---------------------------------------------------------------------------
# nros_canonical_build_type(<raw> <package_xml> <out_var>)
#
# Resolve a `<build_type>` body to its RFC-0087 D2 spelling, warning when the
# authored spelling is retired.
#
# An UNKNOWN value leaves <out_var> empty and is NOT an error: `ament_python`
# and friends are valid ROS 2 build types that simply are not ours, and a
# reader that hard-errors on one cannot be run over a mixed workspace.
# Refusing an unknown value INSIDE this repository is
# `check-build-type-spelling`'s job, where the rule can also see the package's
# class — which a string cannot.
#
# The warning is DEPRECATION (shown by default, unlike AUTHOR_WARNING) and it
# names the file, because all three retired spellings live in test fixtures:
# "some package uses ament_nros" sends the reader grepping 406 package.xml.
# ---------------------------------------------------------------------------
function(nros_canonical_build_type raw package_xml out_var)
    string(STRIP "${raw}" _raw)
    set(${out_var} "" PARENT_SCOPE)
    if(_raw STREQUAL "")
        return()
    endif()
    foreach(_row IN LISTS _NROS_BUILD_TYPE_MAP)
        if(_row MATCHES "^([^=]+)=(.+)$" AND CMAKE_MATCH_1 STREQUAL _raw)
            set(${out_var} "${CMAKE_MATCH_2}" PARENT_SCOPE)
            # `list(FIND)` rather than `if(… IN_LIST …)`: IN_LIST needs CMP0057,
            # which is unset under `cmake -P` (no cmake_minimum_required), and
            # the buildless gates drive this reader in exactly that mode.
            list(FIND _NROS_BUILD_TYPE_RETIRED "${_raw}" _retired_at)
            if(NOT _retired_at EQUAL -1)
                message(DEPRECATION
                    "${package_xml}: <build_type>${_raw}</build_type> is retired "
                    "(RFC-0087 D2) — write <build_type>${CMAKE_MATCH_2}</build_type>. "
                    "It is read as `${CMAKE_MATCH_2}` meanwhile, so nothing breaks "
                    "today; phase-420 W3 removes the spelling.")
            endif()
            return()
        endif()
    endforeach()
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_read_package_export([PACKAGE_XML <path>])
#
# Parse a package.xml's consumption exports (default
# `${CMAKE_CURRENT_SOURCE_DIR}/package.xml`). Sets, in the caller's scope:
#   NANO_ROS_EXPORT_DEPLOY   — deploy attr verbatim (e.g. native / freertos), or ""
#   NANO_ROS_EXPORT_BOARD    — board attr, or ""
#   NANO_ROS_EXPORT_RMW      — rmw attr, or ""
#   NANO_ROS_EXPORT_FOUND    — TRUE iff a <nano_ros …/> element was present
#   NANO_ROS_EXPORT_USES_KINDS      — every selected family, declaration order
#   NANO_ROS_EXPORT_USES_<KIND>     — the name selected for that family
#   NANO_ROS_EXPORT_BUILD_TYPE      — <build_type> in RFC-0087 D2 spelling, or ""
#   NANO_ROS_EXPORT_BUILD_TYPE_RAW  — <build_type> exactly as authored, or ""
#
# Both build-type variables, because they answer different questions: the
# canonical one is what a consumer branches on, the raw one is what a
# diagnostic has to quote back for the user to find the line. An unrecognised
# value leaves the canonical empty while the raw still reports it.
#
# RFC-0087 D3 / phase-420 W1 — two spellings, one meaning:
#
#   <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/>
#   <nano_ros_uses kind="serdes" name="flatbuf"/>
#
# `board=` and `rmw=` are provider selections and desugar into
# `NANO_ROS_EXPORT_USES_{BOARD,RMW}` alongside their legacy variables, so the
# two spellings are indistinguishable to a consumer. `deploy=` does NOT: it
# names a `[deploy.*]` block in system.toml (mapped to the NANO_ROS_PLATFORM
# axis below), not a provider, and folding it in would invent a family with no
# descriptor behind it.
#
# The point of the general form is that **a new provider family costs this
# reader nothing** — selecting a serializer needs no fourth attribute here and
# no new special case in `cargo-nano-ros`'s parser.
#
# A package with no `<nano_ros>` element (or no package.xml) leaves FOUND FALSE
# and the strings empty — callers fall back to their prior defaults.
# ---------------------------------------------------------------------------
function(nano_ros_read_package_export)
    cmake_parse_arguments(_NRP "" "PACKAGE_XML" "" ${ARGN})
    if(NOT _NRP_PACKAGE_XML)
        set(_NRP_PACKAGE_XML "${CMAKE_CURRENT_SOURCE_DIR}/package.xml")
    endif()

    set(NANO_ROS_EXPORT_DEPLOY "" PARENT_SCOPE)
    set(NANO_ROS_EXPORT_BOARD  "" PARENT_SCOPE)
    set(NANO_ROS_EXPORT_RMW    "" PARENT_SCOPE)
    set(NANO_ROS_EXPORT_FOUND  FALSE PARENT_SCOPE)
    set(NANO_ROS_EXPORT_USES_KINDS "" PARENT_SCOPE)
    set(NANO_ROS_EXPORT_BUILD_TYPE "" PARENT_SCOPE)
    set(NANO_ROS_EXPORT_BUILD_TYPE_RAW "" PARENT_SCOPE)

    if(NOT EXISTS "${_NRP_PACKAGE_XML}")
        return()
    endif()
    nros_read_package_xml_body("${_NRP_PACKAGE_XML}" _body)

    # RFC-0087 D2 / phase-420 W2. MATCHALL rather than MATCH: catkin_pkg's
    # `Package.get_build_type()` raises `InvalidPackage` on a second
    # <build_type>, so a file with two of them is already unbuildable by every
    # ROS reader — picking the first here would be this reader inventing a
    # meaning for a file nothing else accepts.
    string(REGEX MATCHALL "<build_type>[^<]*</build_type>" _bts "${_body}")
    list(LENGTH _bts _bt_count)
    if(_bt_count GREATER 1)
        message(FATAL_ERROR
            "${_NRP_PACKAGE_XML}: ${_bt_count} <build_type> elements — only one is "
            "permitted (catkin_pkg raises InvalidPackage on the second)")
    elseif(_bt_count EQUAL 1)
        list(GET _bts 0 _bt)
        string(REGEX REPLACE "^<build_type>(.*)</build_type>$" "\\1" _bt "${_bt}")
        string(STRIP "${_bt}" _bt)
        set(NANO_ROS_EXPORT_BUILD_TYPE_RAW "${_bt}" PARENT_SCOPE)
        nros_canonical_build_type("${_bt}" "${_NRP_PACKAGE_XML}" _bt_canon)
        set(NANO_ROS_EXPORT_BUILD_TYPE "${_bt_canon}" PARENT_SCOPE)
    endif()

    set(_kinds "")

    # The general form first, in declaration order. MATCHALL because a package
    # may select several families; `<nano_ros_uses` cannot be confused with
    # `<nano_ros ` (no whitespace follows the shared prefix) or with
    # `<nano_ros_provides`, which is the opposite direction and read elsewhere.
    string(REGEX MATCHALL "<nano_ros_uses[ \t\r\n]+[^>]*/?>" _uses "${_body}")
    foreach(_use IN LISTS _uses)
        set(_kind "")
        set(_name "")
        if(_use MATCHES "kind[ \t]*=[ \t]*\"([^\"]*)\"")
            set(_kind "${CMAKE_MATCH_1}")
        endif()
        if(_use MATCHES "name[ \t]*=[ \t]*\"([^\"]*)\"")
            set(_name "${CMAKE_MATCH_1}")
        endif()
        if(_kind STREQUAL "" OR _name STREQUAL "")
            message(FATAL_ERROR
                "${_NRP_PACKAGE_XML}: <nano_ros_uses> needs non-empty kind= and name= "
                "(got kind=\"${_kind}\", name=\"${_name}\")")
        endif()
        string(TOUPPER "${_kind}" _KIND)
        set(NANO_ROS_EXPORT_USES_${_KIND} "${_name}" PARENT_SCOPE)
        list(APPEND _kinds "${_kind}")
    endforeach()

    # Then the sugar. Isolate the <nano_ros …/> element (self-closing or
    # paired). Attribute order is free, so pull each attribute independently
    # rather than positionally.
    if(_body MATCHES "<nano_ros[ \t\r\n]+([^>]*)/?>")
        set(_attrs "${CMAKE_MATCH_1}")
        set(NANO_ROS_EXPORT_FOUND TRUE PARENT_SCOPE)

        foreach(_key deploy board rmw)
            if(_attrs MATCHES "${_key}[ \t]*=[ \t]*\"([^\"]*)\"")
                string(TOUPPER "${_key}" _KEY)
                set(NANO_ROS_EXPORT_${_KEY} "${CMAKE_MATCH_1}" PARENT_SCOPE)
                # board= and rmw= ARE provider selections; deploy= is not.
                if(NOT _key STREQUAL "deploy" AND NOT CMAKE_MATCH_1 STREQUAL "")
                    set(NANO_ROS_EXPORT_USES_${_KEY} "${CMAKE_MATCH_1}" PARENT_SCOPE)
                    list(APPEND _kinds "${_key}")
                endif()
            endif()
        endforeach()
    endif()

    list(REMOVE_DUPLICATES _kinds)
    set(NANO_ROS_EXPORT_USES_KINDS "${_kinds}" PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# nano_ros_read_leaf_system([DIR <dir>])
#
# phase-445 W3 (RFC-0098 D3/D5) — a single-package C/C++ leaf states its board,
# RMW, domain and locator in a `system.toml` beside its CMakeLists.txt, the SAME
# schema a workspace bringup uses (`[system] rmw/domain_id/locator`,
# `[image.<id>] board`). That file replaces the `package.xml`
# `<nano_ros deploy= board= rmw=/>` tuple above, which stays readable as the
# deprecated fallback while the remaining leaves are converted.
#
# The file is read by `nros ws leaf-system` — the CLI front of
# `nros_orchestration_ir::leaf_system`, the ONE reader `nros::main!` and
# `nros sync` also use — never by a regex here: a second parser of one file is
# how a Rust and a C leaf would read it two ways. The DEPLOY token (the
# NANO_ROS_PLATFORM axis) is derived from the board through the board catalog.
#
# Call AFTER nano_ros_read_package_export(): on a leaf with a system.toml this
# OVERRIDES the tuple variables that call set, so every consumer downstream
# (the platform/RMW cache writes, the verbs' DEPLOY/BOARD defaults) reads the
# system.toml values through the variables it already reads. Needs
# nros_resolve_cli (NanoRosCodegenCore.cmake).
#
# A leaf carrying BOTH a system.toml and a `<nano_ros …/>` tuple is refused:
# one source per fact.
#
# Sets in the caller's scope:
#   NANO_ROS_LEAF_SYSTEM      — TRUE iff the leaf's system.toml was read
#   NANO_ROS_LEAF_DOMAIN_ID   — `[image] domain_id` > `[system] domain_id`, or ""
#   NANO_ROS_LEAF_LOCATOR     — `[image] locator` > `[system] locator`, or ""
#   NANO_ROS_EXPORT_{DEPLOY,BOARD,RMW,FOUND,USES_*} — overridden, as above
# ---------------------------------------------------------------------------
function(nano_ros_read_leaf_system)
    cmake_parse_arguments(_NRL "" "DIR" "" ${ARGN})
    if(NOT _NRL_DIR)
        set(_NRL_DIR "${CMAKE_CURRENT_SOURCE_DIR}")
    endif()
    set(NANO_ROS_LEAF_SYSTEM FALSE PARENT_SCOPE)
    set(NANO_ROS_LEAF_DOMAIN_ID "" PARENT_SCOPE)
    set(NANO_ROS_LEAF_LOCATOR "" PARENT_SCOPE)
    if(NOT EXISTS "${_NRL_DIR}/system.toml")
        return()
    endif()
    if(NANO_ROS_EXPORT_FOUND)
        message(FATAL_ERROR
            "${_NRL_DIR}/system.toml states this leaf's deployment, and "
            "${_NRL_DIR}/package.xml still carries a <nano_ros deploy=… board=… "
            "rmw=…/> tuple — one source per fact (RFC-0098 D5). Delete the "
            "<nano_ros …/> element from package.xml.")
    endif()

    nros_resolve_cli(_nros CONTEXT "nano_ros_read_leaf_system (${_NRL_DIR}/system.toml)")
    execute_process(
        COMMAND "${_nros}" ws leaf-system "${_NRL_DIR}" --nano-ros-path "${NANO_ROS_ROOT}"
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: ${_NRL_DIR}/system.toml: ${_err}")
    endif()
    # A configure re-runs when the file changes, like package.xml does.
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_NRL_DIR}/system.toml")

    string(REPLACE "\n" ";" _lines "${_out}")
    foreach(_l IN LISTS _lines)
        if(_l MATCHES "^(NROS_LEAF_[A-Z_]+)=(.*)$")
            set(_${CMAKE_MATCH_1} "${CMAKE_MATCH_2}")
        endif()
    endforeach()
    if("${_NROS_LEAF_DEPLOY}" STREQUAL "")
        message(FATAL_ERROR
            "nano-ros: `nros ws leaf-system ${_NRL_DIR}` printed no deploy token:\n${_out}")
    endif()

    # A board that IS its deploy token (`native`, `zephyr`) names no separate
    # provider — exactly the tuple shape `<nano_ros deploy="native"/>` had.
    set(_board "${_NROS_LEAF_BOARD}")
    if(_board STREQUAL _NROS_LEAF_DEPLOY)
        set(_board "")
    endif()

    set(_kinds "${NANO_ROS_EXPORT_USES_KINDS}")
    set(NANO_ROS_EXPORT_FOUND TRUE PARENT_SCOPE)
    set(NANO_ROS_EXPORT_DEPLOY "${_NROS_LEAF_DEPLOY}" PARENT_SCOPE)
    set(NANO_ROS_EXPORT_BOARD "${_board}" PARENT_SCOPE)
    set(NANO_ROS_EXPORT_RMW "${_NROS_LEAF_RMW}" PARENT_SCOPE)
    # board= and rmw= are provider selections (RFC-0087 D3) — desugar them the
    # way the tuple reader does, so the two spellings stay indistinguishable.
    if(NOT _board STREQUAL "")
        set(NANO_ROS_EXPORT_USES_BOARD "${_board}" PARENT_SCOPE)
        list(APPEND _kinds board)
    endif()
    if(NOT "${_NROS_LEAF_RMW}" STREQUAL "")
        set(NANO_ROS_EXPORT_USES_RMW "${_NROS_LEAF_RMW}" PARENT_SCOPE)
        list(APPEND _kinds rmw)
    endif()
    list(REMOVE_DUPLICATES _kinds)
    set(NANO_ROS_EXPORT_USES_KINDS "${_kinds}" PARENT_SCOPE)

    set(NANO_ROS_LEAF_SYSTEM TRUE PARENT_SCOPE)
    set(NANO_ROS_LEAF_DOMAIN_ID "${_NROS_LEAF_DOMAIN_ID}" PARENT_SCOPE)
    set(NANO_ROS_LEAF_LOCATOR "${_NROS_LEAF_LOCATOR}" PARENT_SCOPE)
    message(STATUS
        "nano-ros: deployment from ${_NRL_DIR}/system.toml — board=${_NROS_LEAF_BOARD} "
        "deploy=${_NROS_LEAF_DEPLOY} rmw=${_NROS_LEAF_RMW} "
        "domain_id=${_NROS_LEAF_DOMAIN_ID} locator=${_NROS_LEAF_LOCATOR}")
endfunction()
