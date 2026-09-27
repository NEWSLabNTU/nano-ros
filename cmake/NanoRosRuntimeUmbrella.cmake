# nano-ros — the ONE place that decides which runtime umbrella a target links.
#
# issue 1467. Two rules meet here, and before this module each was spelled
# nine times:
#
#   1. WHICH umbrella. `NanoRos::NanoRosCpp` (== `libnros_cpp.a`) BUNDLES
#      nros-c since Phase 241.D3-rev, so whenever it exists it is the one
#      archive that satisfies both the C and the C++ ABI — preferring it is
#      how a MIXED workspace keeps exactly ONE Rust staticlib on a binary
#      (issue 0425: a C node dragging `libnros_c.a` onto a C++ executable that
#      already had `libnros_cpp.a` produced ~96 duplicate `nros_log_*` /
#      `nros_lifecycle_*` definitions). A pure-C workspace instantiates no
#      `NanoRosCpp` target and falls back to `NanoRos`.
#
#   2. WHETHER the consumer wants one at all. This is the half 0425 did not
#      model, and it is issue 1467. `if(TARGET NanoRos::NanoRosCpp)` is a
#      property of what the build tree DEFINES, not of what the consuming
#      binary's other archives already contain — and every leaf does
#      `add_subdirectory("${NANO_ROS_ROOT}" nano_ros)`, so that target always
#      exists. A leaf whose APP is a Rust staticlib (`nros_threadx_rv64_rust_app`)
#      already carries the whole `nros` / `nros-platform` / `nros-rmw-cffi`
#      surface inside its own archive, so the target that keeps a C/C++ binary
#      down to one archive puts a SECOND archive on a Rust binary: 228
#      `rust-lld: error: duplicate symbol` lines across the twelve
#      `threadx_riscv64` leaves (`nros_rmw_cffi_lookup`,
#      `__NROS_SIZE_EXECUTOR_SIZE`, ~17 more).
#
# Rule 2 is answered by the CONSUMER, through a target property it sets on
# itself (`nros_declare_rust_runtime_carrier`), read at GENERATE time by a
# generator expression on the head target. That is the only mechanism that can
# answer "does THIS binary already have a Rust runtime?" from inside a library
# that was created before the binary existed — the generated message library is
# codegen'd by `nros_generate_interfaces()`, which runs before the entry seam
# declares the executable, and in a workspace may be consumed by several
# binaries with different answers.
#
# WHERE THE GUARD GOES, and where it deliberately does not:
#
#   * a PROPAGATED requirement (PUBLIC / INTERFACE) is guarded. The consumer is
#     unknown at the point of declaration, which is the whole defect.
#   * a PRIVATE link on a binary is NOT guarded, and the umbrella's literal
#     name stays in `LINK_LIBRARIES`. Two reasons, both load-bearing: the
#     consumer IS the target, so the decision is already the consumer's; and
#     `cmake/board/nano-ros-board-{qemu-armv7a,rv-virt}-nuttx.cmake` read that
#     executable's `LINK_LIBRARIES` as literal STRINGS
#     (`if(_lib STREQUAL "NanoRos::NanoRosCpp")`) to skip the umbrella while
#     ferrying its include dirs into the cargo cross-build. A generator
#     expression is not a name those comparisons can match, and the failure
#     would be a silently missing mirror include dir — the Phase 155.B.5
#     `nros_config_generated.h` stub `#error`, several minutes later.
#
# Gate: `check-runtime-umbrella-link-sites` — no site outside this module may
# name an umbrella target in a `target_link_libraries` / `INTERFACE_LINK_LIBRARIES`
# write, so rule 1 cannot gain a tenth spelling and rule 2 cannot be forgotten
# at the next site.
include_guard(GLOBAL)

define_property(TARGET PROPERTY NROS_CARRIES_RUST_RUNTIME
    BRIEF_DOCS
        "This binary already links a Rust staticlib carrying the nano-ros runtime."
    FULL_DOCS
        "Set by `nros_declare_rust_runtime_carrier(<target>)`. When true, a "
        "generated interface library (or any other propagated usage "
        "requirement) does NOT add a runtime umbrella archive "
        "(`libnros_cpp.a` / `libnros_c.a`) to this target's link line: the "
        "target's own Rust staticlib already defines that C ABI, and two "
        "archives defining it is a duplicate-symbol link failure (issue 1467).")

# ---------------------------------------------------------------------------
# nros_declare_rust_runtime_carrier(<target>)
#
# The consumer's half of issue 1467: "my app IS a Rust staticlib, so every
# nano-ros C ABI symbol is already in my own archive." Call it on the
# executable, from the seam that imported the crate.
# ---------------------------------------------------------------------------
function(nros_declare_rust_runtime_carrier target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_declare_rust_runtime_carrier(${target}): no such target.")
    endif()
    set_property(TARGET ${target} PROPERTY NROS_CARRIES_RUST_RUNTIME ON)
endfunction()

# ---------------------------------------------------------------------------
# nros_runtime_umbrella_expr(<out-var> [GUARDED] [CANDIDATES <target>...])
#
# Resolve rule 1 — the first CANDIDATE that exists as a target — and return it
# either bare or wrapped in the rule-2 guard. Returns the empty string when no
# candidate exists (a configure with no umbrella at all; the caller links
# nothing, exactly as the `if(TARGET …)` ladders did).
#
# CANDIDATES defaults to the C/C++ pair. Sites with a narrower ladder pass
# their own (the CPP branch of the codegen has no `NanoRos` fallback; the C
# branch has an extra `nros_c::nros_c` install-time rung).
# ---------------------------------------------------------------------------
function(nros_runtime_umbrella_expr out_var)
    cmake_parse_arguments(_NRU "GUARDED" "" "CANDIDATES" ${ARGN})
    if(NOT _NRU_CANDIDATES)
        set(_NRU_CANDIDATES NanoRos::NanoRosCpp NanoRos::NanoRos)
    endif()
    set(_pick "")
    foreach(_cand IN LISTS _NRU_CANDIDATES)
        if(TARGET ${_cand})
            set(_pick "${_cand}")
            break()
        endif()
    endforeach()
    if(NOT _pick)
        set(${out_var} "" PARENT_SCOPE)
        return()
    endif()
    if(_NRU_GUARDED)
        # The genex condition for "this consumer does NOT carry its own Rust
        # runtime". `$<TARGET_PROPERTY:prop>` with no target name is evaluated
        # against the HEAD target — the binary being linked — which is exactly
        # the question. Measured on cmake 3.22.1 (this tree's minimum) through
        # an ALIAS of an INTERFACE target, a PUBLIC static-library hop and an
        # IMPORTED `_ffi_lib` hop; link ORDER is preserved (the umbrella still
        # lands after the message archive, Phase 150.B).
        #
        # Spelled INSIDE the function, not as a file-scope variable: this module
        # is `include()`d from files that are themselves reachable inside a
        # function frame, and a normal variable set by an `include()` in a
        # function frame is gone when the frame pops (the `_NROS_ENTRY_DIR`
        # pitfall, 287-W6) — while `include_guard(GLOBAL)` means the second
        # include would never re-set it. An empty condition yields the invalid
        # genex `$<:Target>`, i.e. a configure error at some other site.
        set(${out_var}
            "$<$<NOT:$<BOOL:$<TARGET_PROPERTY:NROS_CARRIES_RUST_RUNTIME>>>:${_pick}>"
            PARENT_SCOPE)
    else()
        set(${out_var} "${_pick}" PARENT_SCOPE)
    endif()
endfunction()

# ---------------------------------------------------------------------------
# nros_link_runtime_umbrella(<target> <PUBLIC|PRIVATE|INTERFACE>
#                            [CANDIDATES <target>...])
#
# Link the resolved umbrella at the given scope. PUBLIC / INTERFACE are guarded
# (the requirement propagates to a consumer we do not know); PRIVATE is not
# (see the header block). A configure with no umbrella target links nothing.
# ---------------------------------------------------------------------------
function(nros_link_runtime_umbrella target scope)
    if(NOT scope MATCHES "^(PUBLIC|PRIVATE|INTERFACE)$")
        message(FATAL_ERROR
            "nros_link_runtime_umbrella(${target}): scope must be PUBLIC, "
            "PRIVATE or INTERFACE, got '${scope}'.")
    endif()
    set(_guarded "")
    if(NOT scope STREQUAL "PRIVATE")
        set(_guarded GUARDED)
    endif()
    nros_runtime_umbrella_expr(_expr ${_guarded} ${ARGN})
    if(_expr)
        target_link_libraries(${target} ${scope} "${_expr}")
    endif()
endfunction()
