# NanoRosCodegenCore.cmake — shared helpers for the two nros_generate_interfaces()
# implementations (canonical cmake/NanoRosGenerateInterfaces.cmake +
# zephyr/cmake/nros_generate_interfaces.cmake). Phase 246.
#
# These two generators target genuinely different deployment models (library
# target vs Zephyr `app`, build-time vs configure-time codegen), so they remain
# separate entry points — but the CONTEXT-FREE pieces below were copy-pasted and
# drifted into shipped bugs three times (issues 0052, 0056, Phase 214.B.1). They
# live here now, in one place. Include this from both generators.
#
# Scope note: a `function()`'s `PARENT_SCOPE` reaches only its immediate caller
# (the generator), not the generator's caller (the user). So the helpers that
# must publish a variable to the USER's scope RETURN their result via an out-var
# (landing in the generator's scope); the generator then does the final
# one-line `set(<pkg>_GENERATED_RS_FILES ... PARENT_SCOPE)`. Helpers only read
# enclosing-scope vars (which cascade up) and write the global CACHE (which does
# not).

include_guard(GLOBAL)

# _nros_collect_rs_closure(<out_var> DEPS <pkgs...> OWN <rs-files...>)
#
# Compute the de-duplicated transitive closure of generated FFI `.rs` files:
# each dependency's `<dep>_GENERATED_RS_FILES` (or the `_NROS_PKG_<dep>_*` CACHE
# stash, for multi-level scope chains where PARENT_SCOPE didn't reach) PLUS the
# package's own files. De-dup is REQUIRED: a diamond dependency would otherwise
# carry the same leaf file twice → both the lib.rs `include!()` of it twice
# (Rust E0428, issue 0052) and a doubled closure export. Returns the list in
# <out_var> (in the CALLER's scope).
#
# phase-306 W1 (issue 0253): codegen splits each stem into `<stem>_types.rs`
# (crate-mangled structs + plain field serializers — safe to duplicate across
# per-package crates) and `<stem>_exports.rs` (the `#[no_mangle]` C-ABI
# wrappers). Dependencies contribute their TYPES files ONLY — their exports
# live in their OWN crate/archive — so every package builds its own FFI crate
# and any combination of interface archives links without duplicate
# `nros_cpp_*` definitions. OWN files keep both halves.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRosEdition.cmake")

function(_nros_collect_rs_closure _out_var)
    cmake_parse_arguments(_C "" "" "DEPS;OWN" ${ARGN})
    set(_all "")
    foreach(_dep ${_C_DEPS})
        set(_dep_files "")
        if(DEFINED ${_dep}_GENERATED_RS_FILES)
            set(_dep_files "${${_dep}_GENERATED_RS_FILES}")
        elseif(DEFINED CACHE{_NROS_PKG_${_dep}_GENERATED_RS_FILES})
            set(_dep_files "$CACHE{_NROS_PKG_${_dep}_GENERATED_RS_FILES}")
        endif()
        # Types-only dep contribution (see above). The dep's exported closure
        # carries its own exports (plus transitively-filtered dep types);
        # strip every `_exports.rs` here so only the owning package's crate
        # ever includes them.
        list(FILTER _dep_files EXCLUDE REGEX "_exports\\.rs$")
        list(APPEND _all ${_dep_files})
    endforeach()
    list(APPEND _all ${_C_OWN})
    if(_all)
        list(REMOVE_DUPLICATES _all)
    endif()
    set(${_out_var} "${_all}" PARENT_SCOPE)
endfunction()

# _nros_export_rs_closure(<target> <rs-closure-list>)
#
# Stash the (already de-duplicated) closure in the INTERNAL CACHE under
# `_NROS_PKG_<target>_GENERATED_RS_FILES` so deps generated in a sibling call
# tree can read it when PARENT_SCOPE re-export doesn't reach them (Phase
# 210.E.3). The CACHE write is global, so it is scope-safe to do here; the
# matching `set(<target>_GENERATED_RS_FILES ... PARENT_SCOPE)` must stay in the
# generator body (see the scope note above).
function(_nros_export_rs_closure _target _closure)
    set(_NROS_PKG_${_target}_GENERATED_RS_FILES "${_closure}"
        CACHE INTERNAL "nros cached GENERATED_RS_FILES closure for ${_target}" FORCE)
endfunction()

# _nros_write_ffi_lib_rs(CRATE_SRC <dir> TEMPLATE <ffi_lib_rs.in> RS_FILES <list>
#                        PATH_MODE relative|absolute)
#
# Assemble the FFI crate's `src/lib.rs` from the shared `ffi_lib_rs.in` template:
# one `include!()` per unique generated FFI `.rs` file (skipping `mod.rs`), so all
# cross-package types share one flat module scope. PATH_MODE selects how the
# include path is spelled:
#   relative — emit `file(RELATIVE_PATH …)` from <CRATE_SRC>; portable across
#              clean clones / differing CI paths (Phase 214.B.1). Canonical path.
#   absolute — emit the path verbatim. The Zephyr path uses this (its crate dir
#              and outputs share a binary tree that always co-resolve).
# The template's `@NROS_CPP_FFI_INCLUDES@` placeholder is filled and the result
# written to <CRATE_SRC>/lib.rs. Pure file output — function-scope safe.
function(_nros_write_ffi_lib_rs)
    cmake_parse_arguments(_L "" "CRATE_SRC;TEMPLATE;PATH_MODE" "RS_FILES" ${ARGN})
    if(NOT _L_PATH_MODE STREQUAL "relative" AND NOT _L_PATH_MODE STREQUAL "absolute")
        message(FATAL_ERROR "_nros_write_ffi_lib_rs: PATH_MODE must be relative|absolute, got '${_L_PATH_MODE}'")
    endif()
    set(NROS_CPP_FFI_INCLUDES "")
    foreach(_rs_file ${_L_RS_FILES})
        get_filename_component(_rs_name "${_rs_file}" NAME)
        if(_rs_name STREQUAL "mod.rs")
            continue()
        endif()
        if(_L_PATH_MODE STREQUAL "relative")
            file(RELATIVE_PATH _rs_path "${_L_CRATE_SRC}" "${_rs_file}")
        else()
            set(_rs_path "${_rs_file}")
        endif()
        string(APPEND NROS_CPP_FFI_INCLUDES "include!(\"${_rs_path}\");\n")
    endforeach()
    configure_file("${_L_TEMPLATE}" "${_L_CRATE_SRC}/lib.rs" @ONLY)
endfunction()

# _nros_write_codegen_args_json(ARGS_FILE <path> PACKAGE <name> OUTPUT_DIR <dir>
#     ROS_EDITION <edition> [CODEGEN_CONFIG <path>]
#     INTERFACE_FILES <files...> DEPS <pkgs...>)
#
# Build the `nros codegen --args-file` JSON and write it ONLY when the content
# changed (so a re-configure doesn't perturb the file mtime → the codegen
# add_custom_command / mtime check sees its outputs already up to date,
# essential for the workspace-shared codegen cache). `CODEGEN_CONFIG` is the
# optional RFC-0033 per-field capacity config; omit it to emit no such field.
function(_nros_write_codegen_args_json)
    cmake_parse_arguments(_J ""
        "ARGS_FILE;PACKAGE;OUTPUT_DIR;ROS_EDITION;CODEGEN_CONFIG"
        "INTERFACE_FILES;DEPS" ${ARGN})
    set(_files_json "")
    set(_first TRUE)
    foreach(_f ${_J_INTERFACE_FILES})
        if(NOT _first)
            string(APPEND _files_json ",")
        endif()
        set(_first FALSE)
        string(APPEND _files_json "\n    \"${_f}\"")
    endforeach()
    set(_deps_json "")
    set(_first TRUE)
    foreach(_d ${_J_DEPS})
        if(NOT _first)
            string(APPEND _deps_json ",")
        endif()
        set(_first FALSE)
        string(APPEND _deps_json "\n    \"${_d}\"")
    endforeach()
    set(_cfg_json "")
    if(DEFINED _J_CODEGEN_CONFIG AND NOT _J_CODEGEN_CONFIG STREQUAL "")
        set(_cfg_json ",\n  \"codegen_config\": \"${_J_CODEGEN_CONFIG}\"")
    endif()
    set(_content "{
  \"package_name\": \"${_J_PACKAGE}\",
  \"output_dir\": \"${_J_OUTPUT_DIR}\",
  \"interface_files\": [${_files_json}
  ],
  \"dependencies\": [${_deps_json}
  ],
  \"ros_edition\": \"${_J_ROS_EDITION}\"${_cfg_json}
}
")
    set(_write TRUE)
    if(EXISTS "${_J_ARGS_FILE}")
        file(READ "${_J_ARGS_FILE}" _existing)
        if(_existing STREQUAL _content)
            set(_write FALSE)
        endif()
    endif()
    if(_write)
        file(WRITE "${_J_ARGS_FILE}" "${_content}")
    endif()
endfunction()

# _nros_predict_generated_outputs(<headers_var> <sources_var> <rs_var>
#     LANGUAGE C|CPP PACKAGE <name> OUTPUT_DIR <dir> INTERFACE_FILES <files...>)
#
# Predict the files `nros codegen` will emit for the given interfaces, returning
# three lists (headers / C sources / Rust FFI `.rs`) in the caller's scope.
# CPP: `<pkg>_<kind>_<name>.hpp` + a split `_types.rs`+`_exports.rs` pair per
# part (phase-306 W1: msg→1 pair, srv→request+response, action→goal+result+
# feedback) + the `<pkg>.hpp` umbrella + `mod.rs`. C:
# `<pkg>_<kind>_<name>.{h,c}` + the `<pkg>.h` umbrella. Names are CamelCase→snake,
# package `-`→`_`. The canonical generator feeds these to add_custom_command
# OUTPUT (must match codegen exactly); the Zephyr generator concatenates them for
# its mtime "needs-regen" check.
function(_nros_predict_generated_outputs _hdr_var _src_var _rs_var)
    cmake_parse_arguments(_P "" "LANGUAGE;PACKAGE;OUTPUT_DIR" "INTERFACE_FILES" ${ARGN})
    set(_headers "")
    set(_sources "")
    set(_rs "")
    string(REPLACE "-" "_" _c_pkg "${_P_PACKAGE}")
    foreach(_file ${_P_INTERFACE_FILES})
        get_filename_component(_name "${_file}" NAME_WE)
        get_filename_component(_ext "${_file}" EXT)
        string(REGEX REPLACE "([a-z])([A-Z])" "\\1_\\2" _name_snake "${_name}")
        string(TOLOWER "${_name_snake}" _name_lower)
        if(_ext STREQUAL ".msg")
            set(_kind "msg")
        elseif(_ext STREQUAL ".srv")
            set(_kind "srv")
        elseif(_ext STREQUAL ".action")
            set(_kind "action")
        else()
            message(FATAL_ERROR "_nros_predict_generated_outputs: unknown interface extension '${_ext}' (${_file})")
        endif()
        set(_base "${_P_OUTPUT_DIR}/${_kind}/${_c_pkg}_${_kind}_${_name_lower}")
        if(_P_LANGUAGE STREQUAL "CPP")
            list(APPEND _headers "${_base}.hpp")
            if(_kind STREQUAL "msg")
                set(_parts "${_base}")
            elseif(_kind STREQUAL "srv")
                set(_parts "${_base}_request" "${_base}_response")
            elseif(_kind STREQUAL "action")
                set(_parts "${_base}_goal" "${_base}_result" "${_base}_feedback")
            endif()
            foreach(_part ${_parts})
                list(APPEND _rs "${_part}_types.rs" "${_part}_exports.rs")
            endforeach()
        else()
            list(APPEND _headers "${_base}.h")
            list(APPEND _sources "${_base}.c")
        endif()
    endforeach()
    if(_P_LANGUAGE STREQUAL "CPP")
        list(APPEND _headers "${_P_OUTPUT_DIR}/${_P_PACKAGE}.hpp")
        list(APPEND _rs "${_P_OUTPUT_DIR}/mod.rs")
    else()
        list(APPEND _headers "${_P_OUTPUT_DIR}/${_P_PACKAGE}.h")
    endif()
    set(${_hdr_var} "${_headers}" PARENT_SCOPE)
    set(${_src_var} "${_sources}" PARENT_SCOPE)
    set(${_rs_var} "${_rs}" PARENT_SCOPE)
endfunction()

# _nros_resolve_rust_target(<out_var>)
#
# THE cargo target triple for everything this repo builds through a cmake custom
# command. Never empty — a host build resolves to the host triple, not to "no
# --target".
#
# WHY EXPLICIT-ALWAYS, INCLUDING ON THE HOST (phase-340 W3)
#
# `--target <host-triple>` and no `--target` at all are DIFFERENT cargo
# identities on the same machine. Measured on `nros-core`
# (`--no-default-features --features alloc,std`, `nros-relwithdebinfo`):
#
#   implicit host                        libnros_core-0f6269f7a00e4b29.rlib
#   --target x86_64-unknown-linux-gnu    libnros_core-842ac3b7840799eb.rlib
#
# Same triple, same features, same profile, two compilations. And the compiler
# cache does NOT paper over it: on a private cold sccache, building one spelling
# and then the other gave **0 hits / 7 misses** for `nros-core` and **0 hits /
# 62 misses** for `nros`, where an immediate repeat of the FIRST spelling
# scored 7 and 44 hits. Zero sharing, so this is duplicated CPU and not merely
# duplicated bytes.
#
# Corrosion always passes `--target` — hardcoded, because its whole artifact
# path model is `<target-dir>/<triple>/<profile>/` ("We always set `--target`,
# so that cargo always places artifacts into a directory with the target
# triple", Corrosion.cmake). It is an upstream dependency we do not fork, so
# corrosion's spelling is the fixed point and everything else normalises TO it.
# The alternative was rejected on cost, not taste.
#
# The normalisation is free in work done: `cargo --unit-graph` for `nros-c`
# (`std,rmw-zenoh`) reports 165 units and 160 distinct compilation signatures
# with EITHER spelling. The explicit form only relabels 37 of them from the host
# half to the target half. Its one measured cost is that cargo stops stripping
# debuginfo from the 128 build-graph units (`debuginfo` 0 → 1).
#
# WHY THE CACHE COPY AND NOT JUST `Rust_CARGO_TARGET`
#
# `Rust_CARGO_TARGET` is a NORMAL variable in the scope that called
# `find_package(Corrosion)`, and toolchain helpers publish it with
# `PARENT_SCOPE` — which does not cross an `add_subdirectory()` boundary. A
# generator that reads only the normal variable therefore sees it UNSET in some
# scopes and silently builds for the wrong machine; phase-155 is exactly that
# bug, host x86_64 objects landing in an ARM link. FindRust also writes
# `Rust_CARGO_TARGET_CACHED` as CACHE INTERNAL, and a cache entry is visible
# from every scope, so it is the reliable read.
#
# WHY THE MEMO SITS BELOW THE EXPLICIT TARGET AND NOT ABOVE IT (issue 0553)
#
# `_NROS_RUST_TARGET` is a permanent `CACHE INTERNAL` entry and nothing
# invalidates it, so the version of this function that short-circuited on it
# FIRST let whichever scope called it first decide for the whole build tree —
# forever, across every later reconfigure, because the memo lives in the cache
# rather than in a target dir that a clean rebuild would remove.
#
# `examples/workspaces/realtime-cpp/build-workspace-fixtures-nuttx` was
# configured host-first, so it answered `x86_64-unknown-linux-gnu` while its own
# cache plainly carried `Rust_CARGO_TARGET:STRING=armv7a-nuttx-eabihf`. Two
# distinct failures came out of that one stale string:
#
#   * the message FFI staticlib path is `<target-dir>/<triple>/<profile>/`, so
#     the glue was built and named under `x86_64-unknown-linux-gnu` and the ARM
#     link died on `libnano_ros_cpp_ffi_std_msgs.a: file format not recognized`;
#   * `nros_nuttx_include_root()` derives the NuttX arch from this triple, saw
#     a host triple, matched neither arm nor riscv, and fell back to the shared
#     tree — reintroducing issue 0551 in the one tree that had it worst.
#
# So: an EXPLICIT target always wins, and the memo is consulted only when
# nothing explicit is visible. That keeps what the memo is actually for (not
# re-running `rustc -vV` per call, and giving a scope that cannot see the normal
# variable a consistent answer) while making a stale one unreachable in any
# build that states its triple. Existing poisoned trees self-heal on the next
# configure; no cache wipe is needed.
#
# The corrosion copies stay BELOW the memo deliberately. `Rust_CARGO_TARGET_CACHED`
# was `x86_64-unknown-linux-gnu` in that very tree while the requested target was
# ARM, so promoting them above the memo would let a blind scope overwrite a good
# memo with corrosion's host copy — the same bug facing the other way.
function(_nros_resolve_rust_target _out)
    set(_t "")
    if(DEFINED Rust_CARGO_TARGET AND NOT Rust_CARGO_TARGET STREQUAL "")
        # A toolchain file or an in-scope find_package() said so. Wins outright.
        set(_t "${Rust_CARGO_TARGET}")
    elseif(DEFINED CACHE{Rust_CARGO_TARGET} AND NOT "$CACHE{Rust_CARGO_TARGET}" STREQUAL "")
        # `-DRust_CARGO_TARGET=…` on the configure line. A cache entry is visible
        # from every scope, so this is the reading that survives the
        # `add_subdirectory()` boundary the normal variable does not cross.
        set(_t "$CACHE{Rust_CARGO_TARGET}")
    elseif(DEFINED CACHE{_NROS_RUST_TARGET} AND NOT "$CACHE{_NROS_RUST_TARGET}" STREQUAL "")
        set(${_out} "$CACHE{_NROS_RUST_TARGET}" PARENT_SCOPE)
        return()
    elseif(DEFINED CACHE{Rust_CARGO_TARGET_CACHED} AND NOT "$CACHE{Rust_CARGO_TARGET_CACHED}" STREQUAL "")
        set(_t "$CACHE{Rust_CARGO_TARGET_CACHED}")
    elseif(DEFINED CACHE{_CORROSION_RUST_CARGO_TARGET} AND NOT "$CACHE{_CORROSION_RUST_CARGO_TARGET}" STREQUAL "")
        set(_t "$CACHE{_CORROSION_RUST_CARGO_TARGET}")
    endif()

    if(_t STREQUAL "")
        # No Corrosion in this configure (a pure C++ consumer can reach the
        # codegen path without it). Ask rustc for its own host triple.
        execute_process(
            COMMAND rustc -vV
            OUTPUT_VARIABLE _vv
            RESULT_VARIABLE _rc
            ERROR_QUIET
            OUTPUT_STRIP_TRAILING_WHITESPACE)
        if(_rc EQUAL 0 AND _vv MATCHES "host:[ \t]*([^\n\r]+)")
            string(STRIP "${CMAKE_MATCH_1}" _t)
        endif()
    endif()

    if(_t STREQUAL "")
        # Fail loudly rather than fall back to the implicit spelling: a silent
        # fallback is how this split became invisible in the first place.
        message(FATAL_ERROR
            "nano-ros: cannot determine the cargo target triple. Neither "
            "Rust_CARGO_TARGET nor Corrosion's cached copy is set, and "
            "`rustc -vV` did not report a host. Set -DRust_CARGO_TARGET=<triple>.")
    endif()

    # Rewritten on every resolution that got here, so the memo tracks the
    # authoritative answer instead of freezing the first one (issue 0553).
    set(_NROS_RUST_TARGET "${_t}" CACHE INTERNAL
        "cargo target triple for nano-ros' own cargo custom commands (phase-340 W3)")
    set(${_out} "${_t}" PARENT_SCOPE)
endfunction()

# _nros_ffi_cargo_args(<out_var> MANIFEST <path> TARGET_DIR <path> PROFILE <name>
#     RUST_TARGET <triple> [TARGET_IN_CONFIG] [BUILD_STD <comma-list>])
#
# Assemble the `cargo <args>` for building an FFI staticlib crate (everything
# AFTER the optional `+<toolchain>` prefix, which the caller prepends). Shared
# skeleton: `build --manifest-path … --target-dir …` plus, conditionally:
#   PROFILE     `dev` → no flag (cargo's default debug); `release` → --release;
#               anything else (e.g. nros-relwithdebinfo) → --profile <name>.
#   RUST_TARGET the triple. REQUIRED and non-empty — see below.
#   TARGET_IN_CONFIG
#               the crate's own `.cargo/config.toml` already sets
#               `[build] target`, so do NOT pass `--target` as well (cargo would
#               see it twice). RUST_TARGET is still required, because the
#               ARTIFACT still lands under `<target-dir>/<triple>/<profile>/`
#               and the caller has to spell that path.
#   BUILD_STD   non-empty → -Z build-std=<comma-list> (tier-2/3 embedded triples
#               that ship no precompiled std).
# Toolchain pinning differs per consumer (canonical `+<tc>` prefix + .cargo/
# config.toml; zephyr rust-toolchain.toml), so it stays in each generator.
#
# WHY AN EMPTY RUST_TARGET IS FATAL (phase-340 W3)
#
# It used to mean "host build — omit --target", and that made this helper the
# one place in the repo that could emit cargo's IMPLICIT host spelling. Implicit
# and explicit are different cargo identities on the same machine and share
# nothing, not even through sccache (0 hits / 62 misses, measured) — while
# corrosion, which builds the rest of the same cmake tree, always passes
# `--target`. So "no triple" is not a mode, it is an unanswered question, and
# `_nros_resolve_rust_target()` above answers it for every caller. Failing here
# is what keeps the next generator from quietly re-opening the split.
function(_nros_ffi_cargo_args _out)
    cmake_parse_arguments(_A "TARGET_IN_CONFIG" "MANIFEST;TARGET_DIR;PROFILE;RUST_TARGET;BUILD_STD" "" ${ARGN})
    set(_args build --manifest-path "${_A_MANIFEST}" --target-dir "${_A_TARGET_DIR}")
    if(_A_PROFILE STREQUAL "dev")
        # cargo's default profile — no flag
    elseif(_A_PROFILE STREQUAL "release")
        list(APPEND _args --release)
    elseif(_A_PROFILE)
        list(APPEND _args --profile ${_A_PROFILE})
    endif()
    # Truthiness guards (not `STREQUAL ""`): an omitted/empty one-value keyword
    # leaves _A_<K> UNDEFINED, and `_A_K STREQUAL ""` would then compare the
    # literal string "_A_K" (auto-deref of an unset var is the name) → non-empty
    # → branch fires with an empty value, emitting a bare `--target` / `-Z
    # build-std=`. `if(_A_K)` derefs and treats unset/empty as false.
    if(NOT _A_RUST_TARGET)
        message(FATAL_ERROR
            "_nros_ffi_cargo_args: RUST_TARGET is required and must name a "
            "triple. Resolve it with _nros_resolve_rust_target() — a host "
            "build spells its triple explicitly here, like corrosion does "
            "(phase-340 W3).")
    endif()
    if(NOT _A_TARGET_IN_CONFIG)
        list(APPEND _args --target ${_A_RUST_TARGET})
    endif()
    if(_A_BUILD_STD)
        list(APPEND _args -Z "build-std=${_A_BUILD_STD}")
    endif()
    set(${_out} "${_args}" PARENT_SCOPE)
endfunction()


# nros_resolve_cli(<out_var> [CONTEXT <caller-label>] [OPTIONAL])
#
# THE shared resolver for the `nros` CLI binary — issue #219 retired the four
# divergent hand-written copies (NanoRosEntry, nano_ros_workspace_metadata,
# zephyr nros_system_generate, and the find half of
# `_nros_resolve_codegen_tool` below). One documented precedence order:
#
#   1. `$ENV{NROS_CLI}` — explicit per-invocation override (must EXIST).
#   2. An already-resolved shared codegen-tool cache var
#      (`_NANO_ROS_CODEGEN_TOOL`, then `_NROS_ZEPHYR_CODEGEN_TOOL`) when it
#      holds a real path — one configure resolves the CLI once.
#   3. `find_program`: the environment PATH first (activate.sh wires the
#      in-tree CLI — the sweep-contract SSoT), THEN the provisioned store
#      (`$NROS_HOME/bin`, `~/.nros/bin`) as PATHS fallbacks. PATHS, never
#      HINTS: hints are searched BEFORE PATH, so a stale provisioned
#      `~/.nros/bin/nros` would shadow the in-tree CLI (the museum-CLI trap
#      the zephyr resolver's comment documented — now enforced everywhere).
#
# FATAL when absent unless OPTIONAL (then <out_var> = NOTFOUND). The
# find_program result is cached (`_NROS_CLI_RESOLVED`); a cached path that no
# longer exists is dropped and re-detected.
# _nros_is_zephyr(<out>)
#
# ONE answer to "am I configuring inside a Zephyr build?" (issue 0326).
#
# The naive `if(NANO_ROS_PLATFORM STREQUAL "zephyr")` is a trap.
# `nano_rosConfig.cmake` sets `NANO_ROS_PLATFORM` as a PLAIN, directory-scoped
# variable in whatever scope called `find_package(nano_ros)`, so every
# `add_subdirectory`'d node/component package that does not itself call
# `find_package` evaluates it UNSET — cmake then compares the literal string
# "NANO_ROS_PLATFORM" against "zephyr", the test is false, and the branch
# silently takes the non-Zephyr path. (`if(X)` truthiness is the safe idiom;
# a bare STREQUAL against a possibly-unset var is not.)
#
# Issue 0282 fixed ONE of six identical guards and, in doing so, introduced a
# second spelling of the check — so the tree carried two idioms across six
# sites. This helper is that second idiom, promoted to the single definition,
# and the `ZEPHYR_BASE` arm is why it is preferred over merely promoting
# `NANO_ROS_PLATFORM` to a cache var: it also covers a Zephyr build that never
# routed through nano_rosConfig's platform arm.
function(_nros_is_zephyr _out)
    if(TARGET app AND (DEFINED ZEPHYR_BASE OR NANO_ROS_PLATFORM STREQUAL "zephyr"))
        set(${_out} TRUE PARENT_SCOPE)
    else()
        set(${_out} FALSE PARENT_SCOPE)
    endif()
endfunction()

# phase-400 W5.a — WHERE the per-build nros config headers land.
#
# `nros-{c,cpp}`'s build script writes them under `$CARGO_TARGET_DIR`, and every
# consumer below (three `zephyr_include_directories`, four `OBJECT_DEPENDS` file
# edges) used to spell that as the LITERAL `${CMAKE_BINARY_DIR}/nros-rust`. The
# two agree only because `nros_cargo_build()` happens to choose that path, so
# the literal is a copy of a decision made in another file — and the moment the
# cargo dir moves (W5's shared group) the include path points at a directory
# nothing populates. That is issue 0834's shape: a mirror no re-run repairs.
#
# So the writer publishes `NROS_GENERATED_HEADER_DIR` (`nros_resolve_cargo_dirs()`
# in zephyr/cmake/nros_cargo_build.cmake, resolved ONCE and cached like
# `nros_resolve_knobs()`) and the readers ask for it here. The fallback keeps
# every non-Zephyr caller — and any configure that reaches a consumer before the
# Zephyr module loads — on exactly the path they had.
#
# Note it is deliberately NOT the cargo target dir, which that same function
# resolves alongside it under a different name: W5.c shares the cargo dir across
# images, and these headers must stay per-image when it does.
#
# Only the ROOT workspace matters: these headers come from `nros-c` / `nros-cpp`,
# which are always members of the nros workspace, never of a generated one. The
# `nros-rust-ws-<name>` branch that issue 0616 added for foreign roots produces
# no headers and therefore needs no accessor.
function(_nros_generated_header_dir _out)
    if(NROS_GENERATED_HEADER_DIR)
        set(${_out} "${NROS_GENERATED_HEADER_DIR}" PARENT_SCOPE)
    else()
        set(${_out} "${CMAKE_BINARY_DIR}/nros-rust" PARENT_SCOPE)
    endif()
endfunction()

function(nros_resolve_cli _out)
    cmake_parse_arguments(_RC "OPTIONAL" "CONTEXT" "" ${ARGN})
    if(NOT _RC_CONTEXT)
        set(_RC_CONTEXT "nano-ros")
    endif()
    if(DEFINED ENV{NROS_CLI} AND EXISTS "$ENV{NROS_CLI}")
        set(${_out} "$ENV{NROS_CLI}" PARENT_SCOPE)
        return()
    endif()
    foreach(_cv _NANO_ROS_CODEGEN_TOOL _NROS_ZEPHYR_CODEGEN_TOOL)
        if(DEFINED CACHE{${_cv}} AND ${_cv}
           AND NOT "${${_cv}}" MATCHES "^\\$<" AND EXISTS "${${_cv}}")
            set(${_out} "${${_cv}}" PARENT_SCOPE)
            return()
        endif()
    endforeach()
    if(_NROS_CLI_RESOLVED AND NOT EXISTS "${_NROS_CLI_RESOLVED}")
        message(STATUS "Cached nros CLI no longer exists: ${_NROS_CLI_RESOLVED}; re-detecting")
        unset(_NROS_CLI_RESOLVED CACHE)
    endif()
    set(_paths "$ENV{HOME}/.nros/bin")
    if(DEFINED ENV{NROS_HOME})
        list(PREPEND _paths "$ENV{NROS_HOME}/bin")
    endif()
    find_program(_NROS_CLI_RESOLVED NAMES nros PATHS ${_paths}
        DOC "nros CLI (shared resolver — issue #219)")
    if(_NROS_CLI_RESOLVED)
        set(${_out} "${_NROS_CLI_RESOLVED}" PARENT_SCOPE)
        return()
    endif()
    if(_RC_OPTIONAL)
        set(${_out} "NOTFOUND" PARENT_SCOPE)
        return()
    endif()
    message(FATAL_ERROR
        "${_RC_CONTEXT}: `nros` CLI not found on PATH or in the provisioned "
        "store. nano-ros builds it in-tree from packages/cli/ (Phase 218):\n"
        "  ./scripts/bootstrap.sh && source ./activate.sh   (contributors: just setup-cli)\n"
        "or set $NROS_CLI to an explicit binary.")
endfunction()

# _nros_resolve_codegen_tool(<cache_var_name>)
#
# Ensure the named cache var holds a valid path to the `nros` CLI (the codegen
# tool). Drops a stale cached path (one that no longer EXISTS — but not a
# generator-expression `$<…>` placeholder a cross-compile pre-set may use), then
# find_program on PATH + $NROS_HOME/bin + ~/.nros/bin, FATAL if absent, cache
# INTERNAL. Each generator runs its OWN pre-checks first (zephyr: west `-D`
# pre-set + Kconfig CONFIG_NROS_CODEGEN_TOOL; canonical: profile var) which may
# pre-populate the var — then calls this for the shared find/validate/cache. The
# cache-var name is a PARAMETER because the two trees use distinct names
# (`_NANO_ROS_CODEGEN_TOOL` vs `_NROS_ZEPHYR_CODEGEN_TOOL`, the latter read by
# nros_find_interfaces.cmake) — they must NOT be unified.
function(_nros_resolve_codegen_tool _cv)
    if(${_cv} AND NOT "${${_cv}}" MATCHES "^\\$<" AND NOT EXISTS "${${_cv}}")
        message(STATUS "Cached nros codegen tool no longer exists: ${${_cv}}; re-detecting")
        unset(${_cv} CACHE)
        unset(${_cv})
    endif()
    if(NOT ${_cv})
        # issue #219 — delegate the search to the shared resolver (OPTIONAL:
        # this function owns the richer FATAL text with the -D/Kconfig hints).
        nros_resolve_cli(_found OPTIONAL)
        if(_found)
            set(${_cv} "${_found}")
        endif()
        if(NOT ${_cv})
            message(FATAL_ERROR
                "nros (codegen tool) not found on PATH or in ~/.nros/bin. nano-ros "
                "builds the `nros` CLI in-tree from packages/cli/ (Phase 218):\n"
                "  ./scripts/bootstrap.sh && source ./activate.sh   (contributors: just setup-cli)\n"
                "or pre-set the cache var: -D${_cv}=<path-to-nros> (Zephyr also "
                "accepts prj.conf CONFIG_NROS_CODEGEN_TOOL / west "
                "-D_NANO_ROS_CODEGEN_TOOL=<path>).")
        endif()
        message(STATUS "Found nros codegen tool: ${${_cv}}")
    endif()
    # Cache unconditionally — a caller pre-check may have set the var PLAIN (e.g.
    # zephyr's Kconfig CONFIG_NROS_CODEGEN_TOOL); persist it so a re-configure
    # doesn't lose it. Re-caching an already-cached value is a no-op.
    set(${_cv} "${${_cv}}" CACHE INTERNAL "Path to nros codegen tool" FORCE)
endfunction()

# _nros_resolve_interface_file(<target> <relpath> <out_var> [BUNDLED_PREFIX <p>])
#
# Resolve a ROS interface file in tiers, setting <out_var> (caller scope) to the
# path or NOTFOUND:
#   0. absolute <relpath> (pass through if it EXISTS)
#   1. local      ${CMAKE_CURRENT_SOURCE_DIR}/<relpath>
#   2. ament      <p>/share/<target>/<relpath> for each AMENT_PREFIX_PATH entry
#   3. bundled    <BUNDLED_PREFIX>/share/nano-ros/interfaces/<target>/<relpath>
#                 (only when BUNDLED_PREFIX is given)
# `CMAKE_CURRENT_SOURCE_DIR` is the consumer's directory scope (a function does
# not change it), matching the per-generator resolvers this replaces. The
# bundled tier is opt-in via the prefix so a tree without one simply skips it.
function(_nros_resolve_interface_file target relpath out_var)
    cmake_parse_arguments(_R "" "BUNDLED_PREFIX" "" ${ARGN})
    set(${out_var} "NOTFOUND" PARENT_SCOPE)
    if(IS_ABSOLUTE "${relpath}")
        if(EXISTS "${relpath}")
            set(${out_var} "${relpath}" PARENT_SCOPE)
        endif()
        return()
    endif()
    set(_local "${CMAKE_CURRENT_SOURCE_DIR}/${relpath}")
    if(EXISTS "${_local}")
        set(${out_var} "${_local}" PARENT_SCOPE)
        return()
    endif()
    if(DEFINED ENV{AMENT_PREFIX_PATH})
        string(REPLACE ":" ";" _ament_paths "$ENV{AMENT_PREFIX_PATH}")
        foreach(_prefix ${_ament_paths})
            set(_cand "${_prefix}/share/${target}/${relpath}")
            if(EXISTS "${_cand}")
                set(${out_var} "${_cand}" PARENT_SCOPE)
                return()
            endif()
        endforeach()
    endif()
    if(_R_BUNDLED_PREFIX)
        set(_cand "${_R_BUNDLED_PREFIX}/share/nano-ros/interfaces/${target}/${relpath}")
        if(EXISTS "${_cand}")
            set(${out_var} "${_cand}" PARENT_SCOPE)
            return()
        endif()
        # issue 0663 — tier 4, the SOURCE tree. The bundled tier above names an
        # INSTALLED layout (`share/nano-ros/interfaces/`), which a checkout does
        # not have: in-tree the same files ship at `packages/cli/interfaces/`.
        # So on a host with no ROS, `std_msgs/msg/String.msg` resolved nowhere
        # and every CycloneDDS fixture that generates interfaces through cmake
        # failed — while the files sat in the repo being searched. The compat
        # stub `_NrosFindRosMsgPackage.cmake` already knew this location; the
        # tier list did not.
        set(_cand "${_R_BUNDLED_PREFIX}/packages/cli/interfaces/${target}/${relpath}")
        if(EXISTS "${_cand}")
            set(${out_var} "${_cand}" PARENT_SCOPE)
            return()
        endif()
    endif()
endfunction()

# nros_find_interfaces([PACKAGE_XML <path>] [LANGUAGE C|CPP] [SKIP_INSTALL]
#                      [ROS_EDITION <e>])
#
# High-level package.xml-SSoT entry: read the consumer's package.xml, resolve
# the transitive interface closure via `nros codegen resolve-deps`, then
# `nros_generate_interfaces()` each resolved package in topological order. The
# generate call resolves to WHICHEVER generator the build loaded (canonical =
# standalone lib; zephyr = emit-into-`app`) — the function itself is
# platform-agnostic, which is why it lives in the shared core (Phase 246, was a
# near-identical copy in cmake/NanoRosGenerateInterfaces.cmake and
# zephyr/cmake/nros_find_interfaces.cmake).
#
# DEPRECATED for new code (Phase 210.E.4) — prefer nros_workspace_interfaces()
# for a workspace + upstream-shape find_package(<pkg>) per package. Kept for
# back-compat.
function(nros_find_interfaces)
    cmake_parse_arguments(_ARG "SKIP_INSTALL" "PACKAGE_XML;LANGUAGE;ROS_EDITION" "" ${ARGN})

    if(NOT DEFINED _ARG_PACKAGE_XML OR _ARG_PACKAGE_XML STREQUAL "")
        set(_ARG_PACKAGE_XML "${CMAKE_CURRENT_SOURCE_DIR}/package.xml")
    endif()
    if(NOT EXISTS "${_ARG_PACKAGE_XML}")
        message(FATAL_ERROR "nros_find_interfaces: package.xml not found at ${_ARG_PACKAGE_XML}")
    endif()
    if(NOT DEFINED _ARG_LANGUAGE OR _ARG_LANGUAGE STREQUAL "")
        set(_ARG_LANGUAGE "CPP")
    endif()
    # phase-405 W3 — this used to default straight to the literal, skipping
    # NANO_ROS_ROS_EDITION entirely, so `nros_find_interfaces` ignored a
    # workspace's declared edition while its sibling nros_generate_interfaces
    # honoured it. One workspace, two editions, one build.
    _nros_resolve_ros_edition("${_ARG_ROS_EDITION}" _ARG_ROS_EDITION)

    # Codegen tool: each generator resolved it into its own cache var at include
    # time. Try the Zephyr var first, then the canonical — robust whichever
    # generator is loaded (the two names must stay distinct; see
    # _nros_resolve_codegen_tool).
    set(_codegen_tool "${_NROS_ZEPHYR_CODEGEN_TOOL}")
    if(NOT _codegen_tool)
        set(_codegen_tool "${_NANO_ROS_CODEGEN_TOOL}")
    endif()
    if(NOT _codegen_tool)
        message(FATAL_ERROR
            "nros_find_interfaces: nros codegen tool not resolved — include the "
            "nano-ros interface generator first (NanoRosGenerateInterfaces.cmake "
            "or zephyr/cmake/nros_generate_interfaces.cmake).")
    endif()

    # 1. Resolve the transitive interface closure (configure time). Emits a cmake
    #    script setting `_NROS_RESOLVED_PACKAGES` + per-pkg `_NROS_RESOLVED_<pkg>_FILES`.
    #    Phase 293 / issue #212 — thread the workspace interface search path
    #    (a cmake VAR at the workspace root; the CLI reads the ENV) into the
    #    child so `resolve-deps` sees workspace-local msg packages exactly like
    #    the cmake smart-stub does. A caller-exported ENV wins (matches the
    #    stub's env-beats-nothing layering): only inject when no env is set.
    set(_resolve_env_prefix "")
    if(NOT DEFINED ENV{NROS_INTERFACE_SEARCH_PATH}
       AND DEFINED NROS_INTERFACE_SEARCH_PATH
       AND NOT NROS_INTERFACE_SEARCH_PATH STREQUAL "")
        string(REPLACE ";" ":" _resolve_search_path "${NROS_INTERFACE_SEARCH_PATH}")
        set(_resolve_env_prefix ${CMAKE_COMMAND} -E env
            "NROS_INTERFACE_SEARCH_PATH=${_resolve_search_path}")
    endif()
    set(_resolve_output "${CMAKE_CURRENT_BINARY_DIR}/_nros_resolved_deps.cmake")
    execute_process(
        COMMAND ${_resolve_env_prefix} "${_codegen_tool}" codegen resolve-deps
                --package-xml "${_ARG_PACKAGE_XML}"
                --output-cmake "${_resolve_output}"
        RESULT_VARIABLE _result
        ERROR_VARIABLE _stderr)
    if(NOT _result EQUAL 0)
        message(FATAL_ERROR "nros-codegen resolve-deps failed (exit ${_result}):\n${_stderr}")
    endif()
    include("${_resolve_output}")
    if(NOT _NROS_RESOLVED_PACKAGES)
        message(WARNING "nros_find_interfaces: no interface packages resolved from ${_ARG_PACKAGE_XML}")
        return()
    endif()

    # 2. Generate each resolved package in topo order. Pass ALL already-processed
    #    packages as DEPENDENCIES (a superset of the transitive closure) so the
    #    C++ FFI include!() chain sees every cross-package type; the C path
    #    ignores the surplus.
    #
    # Issue 0277 note: the mixed-subset diagnosis that lived here (two
    # superset archives on one link line) is obsolete by construction under
    # the per-package-crate design — any combination of interface archives
    # links; later find_interfaces calls with new pkgs just build more
    # per-pkg crates. The NROS_FIND_INTERFACES_RESOLVED property and its
    # warning are retired with the superset machinery.
    #    phase-306 W1 (issue 0253): every package builds its OWN FFI crate. The
    #    split types/exports closure (`_nros_collect_rs_closure`) guarantees a
    #    crate exports only its own `nros_cpp_*` symbols — dependency TYPES are
    #    included, dependency EXPORTS are not — so any combination of interface
    #    archives on one link line resolves cleanly. The former topo-last
    #    superset-archive routing (NO_FFI_CRATE) is retired.
    set(_all_preceding_pkgs "")
    foreach(_pkg ${_NROS_RESOLVED_PACKAGES})
        set(_skip "")
        if(_ARG_SKIP_INSTALL)
            set(_skip "SKIP_INSTALL")
        endif()
        nros_generate_interfaces(${_pkg}
            ${_NROS_RESOLVED_${_pkg}_FILES}
            DEPENDENCIES ${_all_preceding_pkgs}
            LANGUAGE ${_ARG_LANGUAGE}
            ROS_EDITION ${_ARG_ROS_EDITION}
            ${_skip})
        # Re-export per-package vars to the caller (canonical sets all of these;
        # the zephyr generator only sets GENERATED_RS_FILES — the rest re-export
        # empty, harmless).
        set(${_pkg}_INCLUDE_DIRS "${${_pkg}_INCLUDE_DIRS}" PARENT_SCOPE)
        set(${_pkg}_LIBRARIES "${${_pkg}_LIBRARIES}" PARENT_SCOPE)
        set(${_pkg}_GENERATED_HEADERS "${${_pkg}_GENERATED_HEADERS}" PARENT_SCOPE)
        set(${_pkg}_GENERATED_SOURCES "${${_pkg}_GENERATED_SOURCES}" PARENT_SCOPE)
        set(${_pkg}_GENERATED_RS_FILES "${${_pkg}_GENERATED_RS_FILES}" PARENT_SCOPE)
        list(APPEND _all_preceding_pkgs "${_pkg}")
    endforeach()
endfunction()
