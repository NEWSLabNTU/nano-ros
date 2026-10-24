# NanoRosConfigHeaderMirror.cmake — the per-build sizes-header MIRROR and every
# edge a consumer needs to it. One home for the class issues 0088 → 0090 → 0114
# → 0122 → 0123 → 0268 → 0740 → 0990 → 1783 kept re-opening one site at a time.
#
#   nros_config_header_mirror(<target> <after-target> <src> <gen-subdir> <name> <dest> ...)
#       the PRODUCER: mirror build.rs's header(s) into an in-tree `include/<subdir>/`
#   nros_config_header_files(<out-var>)
#       every mirrored header the build defines (BOTH crates)
#   nros_config_header_object_depends(<owner-target> <source>...)
#       the CONSUMER edge: order + file-level OBJECT_DEPENDS on every mirror
#   _nros_config_header_stamp(<out-var> <owner-target> <header>...)
#       the local stamp the consumer edge goes through (issue 0740)
#
# ## Why both the mirror and the stamp RE-RUN on every build (issue 1783)
#
# The mirror's SOURCE is a file cargo writes as a side effect of its build
# script (`<cargo-target-dir>/nros-c-generated/rclc/detail/<name>`,
# `<cargo-target-dir>/nros-cpp-generated/rclcpp/detail/<name>`). Neither
# generator has an edge to it — cargo runs behind Corrosion's always-run
# target, and the shared cargo dir is not a file any CMake rule produces. Issue
# 0268 therefore keyed the mirror on a PROXY, `$<TARGET_FILE:nros_{c,cpp}-static>`,
# on the premise that a header that changes comes with an archive that changes.
#
# That premise is false. Measured 2026-10-11 on a codegen-version bump
# (`NROS_CODEGEN_VERSION` 10 → 11): cargo rewrote `nros-c-generated/…/nros_config_generated.h`
# with the new value, rebuilt `libnros_c.a` BYTE-IDENTICAL (the constant is a
# compile-time `#define`, nothing in the archive reads it), and Corrosion's
# `copy_if_different` therefore left the build-dir archive at its OLD mtime.
# Ninja restat'd the proxy as unchanged, the mirror never ran, every message
# TU compiled against the museum header and stopped at its own version
# `#error`. The same holds for any fact the header carries that the archive
# does not encode.
#
# There is no honest proxy for "the header changed" other than the header, and
# the header has no producer CMake can see. So the mirror runs EVERY build
# (after cargo, which already runs every build), and stays cheap and
# rebuild-neutral because it is write-if-changed (`mirror-generated-header.sh`
# copies only on a content difference) and its edge is `restat`: a consumer
# recompiles exactly when the header CONTENT moves. The stamp is the same
# argument one hop further out — its input is a cross-directory file it may
# not name (0740), so it too re-runs and `copy_if_different`s.
#
# "Re-runs every build" is spelled as a FILE input on a SYMBOLIC node (a path
# that never exists, produced by a no-op command), so the command is always out
# of date while its REAL output keeps restat semantics (Ninja) / the post-recipe
# mtime comparison (Make). NOT as a symbolic SECOND OUTPUT of the same command:
# the Makefile generator gives every output after the first a
# `touch_nocreate` recipe (measured, cmake 3.22, on the old two-output nros-cpp
# mirror), which would move the header's mtime on every build and recompile
# every consumer. For the same reason each command has exactly ONE output.
include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosArtifactLayout.cmake")  # issue 1852

# _nros_config_header_rerun_node(<out-var> <path>) — declare (once per
# directory) the always-out-of-date SYMBOLIC node at <path>.
function(_nros_config_header_rerun_node _out _path)
    get_property(_declared DIRECTORY PROPERTY _NROS_CFG_RERUN_NODES)
    if(NOT "${_path}" IN_LIST _declared)
        set_source_files_properties("${_path}" PROPERTIES SYMBOLIC TRUE)
        add_custom_command(OUTPUT "${_path}"
            COMMAND ${CMAKE_COMMAND} -E echo_append ""
            VERBATIM)
        set_property(DIRECTORY APPEND PROPERTY _NROS_CFG_RERUN_NODES "${_path}")
    endif()
    set(${_out} "${_path}" PARENT_SCOPE)
endfunction()

# CACHE INTERNAL, not a normal var: this file is also `include()`d from inside
# functions, whose frame drops normal vars on return (CLAUDE.md pitfall).
set(_NROS_CFG_MIRROR_SH
    "${CMAKE_CURRENT_LIST_DIR}/../scripts/build/mirror-generated-header.sh"
    CACHE INTERNAL "issue 1783: the one writer of the in-tree sizes-header mirror")

# nros_config_header_cargo_manifest(<manifest>) — issue 1904
#
# Record, for `mirror-generated-header.sh`, WHICH subdir of `${CMAKE_BINARY_DIR}/cargo`
# this build's cargo writes: call it beside every `corrosion_import_crate()`
# whose crate writes a mirrored header, with the SAME `MANIFEST_PATH`.
#
# Under a shared cargo dir (`nros_share_corrosion_cargo_dir`) `cargo` is a
# symlink every sharer writes through, and Corrosion puts each import in
# `cargo/<workspace-folder>_<sha1(manifest)[0:5]>`. A second checkout sharing
# the keyed dir writes a second subdir beside this one, and pre-phase-340
# residue (`cargo/build/`, issue 0987) is a third. The mirror used to take the
# NEWEST header across all of them, so a sibling checkout's build made this
# build mirror the sibling's header against its own archive. With the subdir
# recorded, the mirror reads only its own (and REFUSES when only a foreign one
# has the header, rather than taking it).
#
# The name is Corrosion's own formula, Corrosion.cmake `_add_cargo_build`
# (`cargo_folder_name` + `cargo_path_hash`, pinned v0.6.1-nros1 lines 778-784):
# Corrosion exposes the directory through no property, variable or argument
# (issue 0945 checked v0.6.1 and upstream `master`). Re-deriving it here is
# safe in the direction that matters: if Corrosion ever renames it, the
# recorded subdir holds no header, the mirror finds a header only in a subdir
# it was not told about, and it FAILS naming this issue — loud, never a silent
# wrong copy.
#
# The record is configure-scoped: the first call in a configure truncates it,
# so a subdir a previous configure named does not linger.
function(nros_config_header_cargo_manifest _manifest)
    cmake_path(GET _manifest PARENT_PATH _parent)
    cmake_path(GET _parent PARENT_PATH _grandparent)
    string(REPLACE "${_grandparent}/" "" _folder "${_parent}")
    string(SHA1 _hash "${_manifest}")
    string(SUBSTRING "${_hash}" 0 5 _hash)
    set(_name "${_folder}_${_hash}")
    set(_record "${CMAKE_BINARY_DIR}/CMakeFiles/nros-cargo-subdirs.txt")
    get_property(_seen GLOBAL PROPERTY _NROS_CARGO_SUBDIRS_RECORDED)
    if(NOT DEFINED _seen OR "${_seen}" STREQUAL "")
        file(WRITE "${_record}" "${_name}\n")
        set_property(GLOBAL PROPERTY _NROS_CARGO_SUBDIRS_RECORDED "${_name}")
    elseif(NOT "${_name}" IN_LIST _seen)
        file(APPEND "${_record}" "${_name}\n")
        set_property(GLOBAL APPEND PROPERTY _NROS_CARGO_SUBDIRS_RECORDED "${_name}")
    endif()
endfunction()

set(_NROS_CFG_PLACER_PY
    "${CMAKE_CURRENT_LIST_DIR}/../scripts/build/cargo-out-dir-headers.py"
    CACHE INTERNAL "issue 1824: the one placer of build-script headers from \$OUT_DIR")

# nros_config_header_place(<corrosion-target> <dest-dir> <package>...) — issue 1824
#
# Run this import's cargo UNDER `cargo-out-dir-headers.py`, so the build
# script's headers are copied from cargo's own `$OUT_DIR` into <dest-dir>, a
# directory THIS configure names and owns. `$OUT_DIR` is per unit, hashed by
# cargo, and reported on the JSON stream even on a fully cached run (measured
# for `cargo rustc`, the verb Corrosion uses: 0 units rebuilt, both
# `build-script-executed` events present). So the header comes from the cargo
# run this build just made — never from `$CARGO_TARGET_DIR/nros-*-generated/`,
# the unowned side channel a shared target dir lets another leaf write
# (issues 0805, 0978, 0987, 1904). The Zephyr lane has placed this way since
# phase-400 W5.c; this is the same script, so the stream has one reader.
#
# The seam is Corrosion's per-target `INTERFACE_CORROSION_CARGO`, never
# `Rust_CARGO`: that would put every crate of the configure under the placer,
# user crates that write no header included. It reaches the BUILD command and
# `cargo-clean_<crate>` only — Corrosion's configure-time `cargo metadata` runs
# `_CORROSION_CARGO` directly. The property is one path (`cargo-clean_` does
# not expand lists), so it names a generated wrapper; the real cargo inside it
# is `_CORROSION_CARGO`, exactly what Corrosion would have run.
#
# `--message-format` goes through `corrosion_set_cargo_flags`, which Corrosion
# places before its rustc `--` delimiter; the clean command does not carry it,
# and the placer passes a command without it through untouched.
#
# <package>... names every crate whose build script writes a header into this
# build: `nros-c` for the C staticlib; `nros-cpp` AND `nros-c` for the C++ one,
# which links nros-c and mirrors its C header too (nros-cpp's own build script
# writes only the C++ one).
#
# <dest-dir> receives `<dest-dir>/nros-{c,cpp}-generated/<subdir>/<name>`; read
# it with `nros_config_header_placed()` rather than spelling that path.
function(nros_config_header_place _target _dest)
    if(NOT ARGN)
        message(FATAL_ERROR "nros_config_header_place(${_target}): no package named")
    endif()
    if(NOT TARGET ${_target})
        message(FATAL_ERROR "nros_config_header_place: no Corrosion target ${_target}")
    endif()
    if(NOT _CORROSION_CARGO)
        message(FATAL_ERROR
            "nros_config_header_place(${_target}): _CORROSION_CARGO is unset — "
            "call it after corrosion_import_crate()")
    endif()
    file(MAKE_DIRECTORY "${_dest}")
    set(_pkg_args "")
    foreach(_p IN LISTS ARGN)
        string(APPEND _pkg_args "--package \"${_p}\" ")
    endforeach()
    set(_wrapper "${CMAKE_CURRENT_BINARY_DIR}/CMakeFiles/${_target}-cargo-placer.sh")
    file(WRITE "${_wrapper}"
        "#!/bin/sh\n"
        "# Generated by nros_config_header_place (issue 1824) — do not edit.\n"
        "exec python3 \"${_NROS_CFG_PLACER_PY}\" ${_pkg_args}"
        "--dest \"${_dest}\" -- \"${_CORROSION_CARGO}\" \"$@\"\n")
    file(CHMOD "${_wrapper}" PERMISSIONS
        OWNER_READ OWNER_WRITE OWNER_EXECUTE GROUP_READ GROUP_EXECUTE
        WORLD_READ WORLD_EXECUTE)
    set_property(TARGET ${_target} PROPERTY INTERFACE_CORROSION_CARGO "${_wrapper}")
    corrosion_set_cargo_flags(${_target} --message-format=json-render-diagnostics)
endfunction()

# nros_config_header_placed(<out-var> <dest-dir> <lang> <name>) — where
# `nros_config_header_place` puts <name> of language <lang> (`c` / `cpp`).
function(nros_config_header_placed _out _dest _lang _name)
    nros_generated_subpath(_sub ${_lang})
    set(${_out} "${_dest}/${_sub}/${_name}" PARENT_SCOPE)
endfunction()

# nros_config_header_mirror(<target> <after-target> [<src> <gen-subdir> <name> <dest>]...)
#
# <src>        the header to mirror: since issue 1824 the copy
#              `nros_config_header_place` put under its dest dir, with
#              <gen-subdir> `-`; otherwise build.rs's leaf copy (the corrosion
#              build dir), the fallback
# <gen-subdir> `nros-c-generated/rclc/detail` / `nros-cpp-generated/rclcpp/detail` under the
#              cargo dir (`nros_generated_subpath`)
# <name>       header file name
# <dest>       the mirror path consumers include
#
# Declares the OUTPUT custom command for every <dest> plus the custom target
# <target> consumers order on. <after-target> is the cargo build (order only:
# the archive is NOT an input — see the file header).
function(nros_config_header_mirror _target _after)
    list(LENGTH ARGN _n)
    math(EXPR _rem "${_n} % 4")
    if(_n EQUAL 0 OR NOT _rem EQUAL 0)
        message(FATAL_ERROR
            "nros_config_header_mirror(${_target}): headers come in groups of four "
            "(<src> <gen-subdir> <name> <dest>), got ${_n} argument(s)")
    endif()
    # issue 1783 — always out of date (see the file header).
    _nros_config_header_rerun_node(_rerun
        "${CMAKE_CURRENT_BINARY_DIR}/CMakeFiles/${_target}.rerun")
    set(_after_dep "")
    if(TARGET ${_after})
        set(_after_dep ${_after})
    endif()
    set(_outs "")
    math(EXPR _last "${_n} - 1")
    foreach(_i RANGE 0 ${_last} 4)
        math(EXPR _j1 "${_i} + 1")
        math(EXPR _j2 "${_i} + 2")
        math(EXPR _j3 "${_i} + 3")
        list(GET ARGN ${_i} _src)
        list(GET ARGN ${_j1} _gendir)
        list(GET ARGN ${_j2} _name)
        list(GET ARGN ${_j3} _dest)
        get_filename_component(_dest_dir "${_dest}" DIRECTORY)
        file(MAKE_DIRECTORY "${_dest_dir}")
        # issue 0805/0978 — the script picks the freshest of the shared
        # cargo copy and the leaf copy, and writes only on a difference.
        add_custom_command(
            OUTPUT "${_dest}"
            COMMAND bash "${_NROS_CFG_MIRROR_SH}"
                "${_src}" "${CMAKE_BINARY_DIR}" "${_gendir}" "${_name}" "${_dest}"
            DEPENDS "${_rerun}" ${_after_dep}
            COMMENT "nano-ros: mirroring ${_name} for in-tree consumers"
            VERBATIM)
        list(APPEND _outs "${_dest}")
    endforeach()
    add_custom_target(${_target} DEPENDS ${_outs})
endfunction()

# nros_config_header_files(<out-var>) — every mirrored sizes header this build
# defines, from BOTH crates. A consumer stamps all of them, because which one a
# TU actually reads is decided by the INCLUDE PATH, not by the consumer's
# language: a generated C message library linked through `NanoRosCpp` resolves
# `<rclc/detail/nros_config_generated.h>` to the nros-CPP mirror (its dir is prepended
# `BEFORE`), and before issue 1783 that library named only the nros-c one.
function(nros_config_header_files _out)
    get_property(_c   GLOBAL PROPERTY NROS_C_CONFIG_HEADER_FILE)
    get_property(_cpp GLOBAL PROPERTY NROS_CPP_CONFIG_HEADER_FILE)
    set(${_out} ${_c} ${_cpp} PARENT_SCOPE)
endfunction()

# nros_config_header_object_depends(<owner-target> <source>...) — the consumer
# edge, in one spelling: target-level order on both mirror targets, plus a
# file-level OBJECT_DEPENDS (through the local stamp) on every listed source.
# A no-op where the build defines no mirror (Zephyr, NuttX, the freertos
# carrier — they generate the header on other paths).
function(nros_config_header_object_depends _owner)
    nros_config_header_files(_hdrs)
    if(NOT _hdrs)
        return()
    endif()
    if(TARGET ${_owner})
        foreach(_dep nros_c_config_header nros_cpp_config_header)
            if(TARGET ${_dep})
                add_dependencies(${_owner} ${_dep})
            endif()
        endforeach()
    endif()
    if(ARGN)
        _nros_config_header_stamp(_stamps "${_owner}" ${_hdrs})
        set_source_files_properties(${ARGN} PROPERTIES OBJECT_DEPENDS "${_stamps}")
    endif()
endfunction()

# ---------------------------------------------------------------------------
# _nros_config_header_stamp(<out-var> <owner-target> <header>...)  — issue 0740,
#                                    keyed per target by 0990, re-run by 1783
#
# A LOCAL proxy for cross-directory generated headers, so a file-level
# `OBJECT_DEPENDS` works under the Unix Makefiles generator.
#
# The mirrored config headers are `add_custom_command(OUTPUT ...)` products of
# `packages/api/nros-{c,cpp}/`. Ninja keeps one global graph, so a consumer in
# another directory can name the file and get the edge. Make does not: a custom
# command's OUTPUT rule exists only in the makefile of the directory that
# declared it, so a consumer elsewhere names a prerequisite nothing can build —
#
#     No rule to make target '.../nros-c/include/rclc/detail/nros_config_generated.h',
#     needed by '.../<entry>_nros_main_generated.cpp.o'.
#
# `add_dependencies` does NOT fix this, and that is the trap: it was already
# there (`_nros_node_register_apply_config_header_deps`) when issue 0740 was
# filed. Target-level ordering says "build that target first"; it does not give
# the .o's prerequisite a RULE. Only the second build passes, because by then
# the file exists — which is why in-tree lanes never saw it and a clean
# downstream consumer build always does.
#
# So give the CONSUMER's own directory a rule. The stamp is a `copy_if_different`
# of the headers, which means:
#
#   * Make has a local rule, so the prerequisite resolves on a clean tree;
#   * `DEPENDS` names the producing TARGETS (legal, and the ordering edge);
#   * the stamp's mtime moves only when the header CONTENT does, so the
#     rebuild-on-change edge issues 0088/0268 exist for is preserved. A bare
#     `touch` stamp would order correctly and rebuild every consumer TU on every
#     build, which is how a correctness fix becomes a build-time regression.
#
# Issue 1783 — and the stamp RE-RUNS every build. Its input is the mirror, a
# file in another directory it may not name (above), so until 1783 it went
# stale on a proxy: the staticlib FILE, which does not change when only the
# header does (see the file header). The copy is `copy_if_different`, so
# re-running it costs one `cmp` and moves no mtime unless the content moved.
function(_nros_config_header_stamp _out_var _owner)
    set(_stamps "")
    # Order only: the mirror targets (and the cargo builds behind them) run
    # first. No FILE input — the stamp re-runs regardless (issue 1783).
    set(_deps "")
    foreach(_t cargo-build_nros_c cargo-build_nros_cpp
               nros_c_config_header nros_cpp_config_header
               nros_c_cargo_build nros_cpp_cargo_build)
        if(TARGET ${_t})
            list(APPEND _deps ${_t})
        endif()
    endforeach()
    foreach(_hdr ${ARGN})
        # One stamp per header, rather than one stamp for all of them:
        # `cmake -E copy_if_different` takes many sources only with a DIRECTORY
        # destination, and concatenating would need a shell redirect that
        # `add_custom_command` cannot express portably. Per-header also keeps
        # `copy_if_different`'s exact semantics — a header that did not change
        # does not touch its stamp.
        #
        # Keyed by the header's crate dir as well as its stem: nros-c and
        # nros-cpp each mirror a `nros_config_generated.h`, and both are stamped
        # since issue 1783.
        get_filename_component(_stem "${_hdr}" NAME_WE)
        # The crate is the directory above `include/`, however deep the header
        # sits under it: `include/nros/` for C++, `include/rclc/detail/` for C
        # (phase-501 W2), so counting parents would name a different dir per
        # language.
        if(_hdr MATCHES "/([^/]+)/include/[^ ]+$")
            set(_crate "${CMAKE_MATCH_1}")
        else()
            message(FATAL_ERROR
                "_nros_config_header_stamp: ${_hdr} is not under a `<crate>/include/` dir")
        endif()
        # Per CONSUMING TARGET, not per directory — issue 0990.
        #
        # A custom command whose OUTPUT is reached through `OBJECT_DEPENDS` has
        # no owning target, so the Makefile generator emits its rule into the
        # build.make of EVERY target that consumes it. That duplication is what
        # makes issue 0740's fix work (the consumer's own directory gets a rule
        # for the prerequisite), and it cannot be removed without reintroducing
        # 0740 — but with a shared OUTPUT path it also gives N independent make
        # rules writing ONE file. Measured in
        # `examples/workspaces/features/build/posix-zenoh-linux/cmake`: 16
        # targets each declaring `_nros_cfg_stamp/nros_config_generated.stamp`.
        # Under `make -j` several can run `copy_if_different` on it at once, and
        # a failure in any one makes GNU make delete the file the others just
        # wrote.
        #
        # Keying the path by target keeps every property that mattered — a rule
        # local to the consumer, and `copy_if_different` so the stamp's mtime
        # still moves only when the header CONTENT does — while giving each rule
        # an output nothing else writes. The cost is one small copy per entry.
        set(_dir "${CMAKE_CURRENT_BINARY_DIR}/_nros_cfg_stamp/${_owner}")
        set(_stamp "${_dir}/${_crate}-${_stem}.stamp")
        _nros_config_header_rerun_node(_rerun "${_dir}/rerun")
        list(APPEND _stamps "${_stamp}")
        # Idempotent per (directory, header): several entries in one
        # CMakeLists.txt share one rule, and declaring it twice is an error.
        get_property(_declared DIRECTORY PROPERTY _NROS_CFG_STAMPS)
        if(NOT "${_stamp}" IN_LIST _declared)
            add_custom_command(
                OUTPUT "${_stamp}"
                COMMAND ${CMAKE_COMMAND} -E make_directory "${_dir}"
                COMMAND ${CMAKE_COMMAND} -E copy_if_different "${_hdr}" "${_stamp}"
                DEPENDS "${_rerun}" ${_deps}
                COMMENT "nano-ros: config-header stamp ${_crate}/${_stem} (issues 0740, 1783)"
                VERBATIM)
            set_property(DIRECTORY APPEND PROPERTY _NROS_CFG_STAMPS "${_stamp}")
        endif()
    endforeach()
    set(${_out_var} "${_stamps}" PARENT_SCOPE)
endfunction()
