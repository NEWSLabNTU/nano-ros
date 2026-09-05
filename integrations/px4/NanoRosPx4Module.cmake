# NanoRosPx4Module.cmake — link nano-ros into a PX4 module (phase-325 W1.2).
#
# Usage, from a module's CMakeLists inside <EXTERNAL_MODULES_LOCATION>/src/modules/<name>/:
#
#     include(${NROS_REPO_DIR}/integrations/px4/NanoRosPx4Module.cmake)
#
#     nros_px4_add_module(
#         MODULE   modules__my_module
#         MAIN     my_module
#         BACKENDS uorb
#         SRCS     ${CMAKE_CURRENT_LIST_DIR}/MyModule.cpp
#     )
#
# Every argument except BACKENDS is forwarded to px4_add_module() untouched, so
# INCLUDES / DEPENDS / COMPILE_FLAGS / STACK_MAIN all behave exactly as PX4
# documents them. This wraps the LINK step; it does not reinvent PX4's module
# factory.
#
# ---------------------------------------------------------------------------
# Why this is a LINK helper and not `find_package(nano_ros)`
# ---------------------------------------------------------------------------
#
# `find_package(nano_ros)` → `_nros_bootstrap` works by `add_subdirectory`, which
# compiles nano-ros sources INSIDE PX4's cmake — where they inherit PX4's flags:
#
#     -Werror -Wfatal-errors -Wpedantic -Wnested-externs -Wbad-function-cast
#     -Wshadow -Wdouble-promotion -Wfloat-equal -Wlogical-op ...
#
# That set is far stricter than nano-ros's own, and `nros-platform-posix` does not
# survive it: every TU dies on `"_DEFAULT_SOURCE" redefined [-Werror]`, PX4 having
# already defined it (phase-325 W1.1, measured). Fixing that one macro would only
# buy the next warning.
#
# So each project builds its own artifacts under its own warning policy, and PX4
# links the results. `libnros_cpp.a` already worked this way — cargo builds it,
# cmake only links it — and the platform shim now follows the same rule.
#
# ---------------------------------------------------------------------------
# Prerequisites — build these first; this module only LINKS them
# ---------------------------------------------------------------------------
#
#   cargo build -p nros-cpp --no-default-features --features std,rmw-cffi --release
#   cmake -S packages/platform/nros-platform-posix -B build/nros-platform-posix
#   cmake --build build/nros-platform-posix
#
# Override either path with -DNROS_CPP_ARCHIVE=... / -DNROS_PLATFORM_ARCHIVE=...
# or the matching env vars. Note PX4's Makefile does NOT forward EXTRA_CMAKE_ARGS,
# so from a `make px4_sitl_default` invocation the environment is the way in.

include_guard(GLOBAL)

# --- Resolve the nano-ros checkout ------------------------------------------
if(NOT DEFINED NANO_ROS_ROOT OR NANO_ROS_ROOT STREQUAL "")
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        # <root>/integrations/px4/ → up two.
        get_filename_component(NANO_ROS_ROOT "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
    endif()
endif()

# --- Resolve the two prebuilt archives --------------------------------------
function(_nros_px4_resolve_archive OUT_VAR CACHE_VAR ENV_VAR DEFAULT_PATH BUILD_HINT)
    if(DEFINED ${CACHE_VAR} AND NOT "${${CACHE_VAR}}" STREQUAL "")
        set(_p "${${CACHE_VAR}}")
    elseif(NOT "$ENV{${ENV_VAR}}" STREQUAL "")
        set(_p "$ENV{${ENV_VAR}}")
    else()
        set(_p "${DEFAULT_PATH}")
    endif()
    if(NOT EXISTS "${_p}")
        # Fail at CONFIGURE time with the command that produces it. The
        # alternative — a missing-symbol wall at link time, 1100 targets later —
        # names none of this.
        message(FATAL_ERROR
            "nros_px4_add_module: required nano-ros archive not found:\n"
            "    ${_p}\n"
            "Build it first:\n"
            "    ${BUILD_HINT}\n"
            "or point at an existing one with -D${CACHE_VAR}=<path> / ${ENV_VAR}=<path>.")
    endif()
    set(${OUT_VAR} "${_p}" PARENT_SCOPE)
endfunction()

_nros_px4_resolve_archive(_NROS_PX4_CPP_A NROS_CPP_ARCHIVE NROS_CPP_ARCHIVE
    "${NANO_ROS_ROOT}/target/release/libnros_cpp.a"
    # Issue 0436 — a MULTI-RMW module (a uORB->RMW bridge) additionally needs the
    # `bridge` feature, which is what puts `nros_init_multi` / `nros_pubsub_bridge_*`
    # (the ABI `<nros/bridge.hpp>`'s MultiExecutor calls) into this archive:
    #   cargo build -p nros-cpp --no-default-features \
    #       --features std,rmw-zenoh-cffi,bridge --release
    "cargo build -p nros-cpp --no-default-features --features std,rmw-cffi --release")

_nros_px4_resolve_archive(_NROS_PX4_PLATFORM_A NROS_PLATFORM_ARCHIVE NROS_PLATFORM_ARCHIVE
    "${NANO_ROS_ROOT}/build/nros-platform-posix/libnros_platform_posix.a"
    "cmake -S ${NANO_ROS_ROOT}/packages/platform/nros-platform-posix -B ${NANO_ROS_ROOT}/build/nros-platform-posix && cmake --build ${NANO_ROS_ROOT}/build/nros-platform-posix")

# The NETWORKED backends this helper knows, in ONE place. Three sites read it —
# the BACKENDS dispatch, the unknown-backend error, and the archive precheck
# below — and when it was spelled out at each, the precheck's copy is the one
# that would have been forgotten.
set(_NROS_PX4_NETWORKED_BACKENDS zenoh xrce cyclonedds)

# Does ${_nm_out} (llvm-nm --defined-only output) define nros_rmw_<backend>_register?
#
# Bounded on both sides rather than a bare substring, because the archive also
# carries Rust-mangled names that CONTAIN the C one's prefix — measured on a
# `rmw-zenoh-cffi` archive:
#
#   nros_rmw_zenoh_register                                    <- the C symbol
#   _RNvNtCs..._14nros_rmw_zenoh13cffi_register8register       <- not it
#   _RNvNvNtCs..._14nros_rmw_zenoh13cffi_register1__32___nros_rmw_backend_auto_register
#
# One predicate for BOTH directions of the check below: a presence test and an
# absence test that disagree about what "defines" means is how a guard ends up
# right in one direction and blind in the other.
function(_nros_px4_archive_defines_backend OUT_VAR NM_OUT BACKEND)
    if("\n${NM_OUT}\n" MATCHES "[^A-Za-z0-9_]nros_rmw_${BACKEND}_register[^A-Za-z0-9_]")
        set(${OUT_VAR} TRUE PARENT_SCOPE)
    else()
        set(${OUT_VAR} FALSE PARENT_SCOPE)
    endif()
endfunction()

# nros-cpp / nros-c live under packages/api (phase-321 moved them there from
# packages/core); the ABI headers stayed in packages/core. Validated below rather
# than trusted: the W1 probe declared its symbols by hand instead of including
# headers — deliberately, to isolate "does it link" — so a wrong include path
# here would have linked green and only failed for the first caller that included
# something. Which is exactly how this file was born with packages/core paths.
set(_NROS_PX4_INCLUDES
    "${NANO_ROS_ROOT}/packages/api/nros-cpp/include"
    "${NANO_ROS_ROOT}/packages/api/nros-c/include"
    "${NANO_ROS_ROOT}/packages/core/nros-rmw-abi/include"
    "${NANO_ROS_ROOT}/packages/platform/nros-platform-api/include")

# The PER-BUILD generated headers (storage sizes, feature constants), emitted by
# nros-c/nros-cpp build.rs into $CARGO_TARGET_DIR/nros-{c,cpp}-generated/nros/.
# Without these, `#include <nros/init.h>` hits the checked-in STUB, whose whole
# body is:
#
#     #error "nros_config_generated.h must be supplied per-build by the build system"
#
# These MUST come from the same cargo invocation that produced libnros_cpp.a —
# they carry storage sizes, and a header describing different sizes than the
# archive was compiled with is the issue-0268 silent-overflow class, which fails
# at runtime rather than at build.
# PREPENDED, not appended. `packages/api/nros-c/include/nros/` ships a checked-in
# STUB of the same name whose body is the #error above; if that directory is
# searched first, the stub wins and the real header is never seen. Include order
# is the whole mechanism here.
list(PREPEND _NROS_PX4_INCLUDES
    "${NANO_ROS_ROOT}/target/nros-c-generated"
    "${NANO_ROS_ROOT}/target/nros-cpp-generated")

foreach(_inc IN LISTS _NROS_PX4_INCLUDES)
    if(NOT IS_DIRECTORY "${_inc}")
        message(FATAL_ERROR
            "NanoRosPx4Module.cmake: include dir does not exist:\n    ${_inc}\n"
            "A package probably moved. Fix _NROS_PX4_INCLUDES — a stale include "
            "path here fails only for callers that include a header, long after "
            "the link succeeded.")
    endif()
endforeach()

# --- The generated headers must PAIR with the archive -----------------------
#
# The loop above asks a directory question, which is the right question for the
# four SOURCE include dirs (does this package still live here?) and the WRONG one
# for the two generated dirs. Issue 1046: `IS_DIRECTORY` cannot tell *present*
# from *current*, and a generated dir outlives every build that wrote into it.
# Measured in the shared checkout on 2026-09-04, with no build running:
# `target/nros-cpp-generated/nros/nros_cpp_config_generated.h` present and
# naming a zenoh variant, `target/release/libnros_cpp.a` absent. Both include
# dirs passed the loop above.
#
# Issue 1050's link failure is exactly this pair disagreeing:
#     undefined reference to `nros_cpp_config_variant_alloc_env_platform_posix_
#         rmw_cffi_rmw_zenoh_cffi_ros_humble_std'
# i.e. the module compiled against a header describing an archive it was not
# linked against. That is the anchor doing its job at LINK time, ~1100 targets
# in. Below it is the same fact, at configure time, naming the command.
#
# The archive path is overridable (`NROS_CPP_ARCHIVE`) and the header path is
# not — it is derived from NANO_ROS_ROOT — so an override silently un-pairs
# them. That hole is permanent by construction and is the main thing this catches
# that a recipe fix cannot: `just px4 build-sitl-example` now builds the archive
# it links (issue 1050), but it can only speak for the DEFAULT path.
include(${CMAKE_CURRENT_LIST_DIR}/NanoRosArchivePairing.cmake)

# ONE hint for both calls, and it names BOTH feature sets on purpose. The header
# and the archive are the same cargo byproduct, so the fix is always "rebuild the
# umbrella" — but WHICH features depends on the module, and a hint that named only
# the uORB set would send a bridge developer to the command that breaks their
# link (issue 0436: `bridge` is what puts nros_init_multi / nros_pubsub_bridge_*
# in, and `rmw-<net>-cffi` is what makes the outward backend exist at all). At
# file scope BACKENDS is not yet known, so the hint says both rather than
# guessing one.
set(_NROS_PX4_REBUILD_HINT
    "cargo build -p nros-cpp --no-default-features --features std,rmw-cffi,platform-posix --release\n"
    "    (a uORB->RMW BRIDGE module instead wants: --features std,rmw-<zenoh|xrce|cyclonedds>-cffi,bridge,platform-posix)\n"
    "    Whichever you run, it rewrites the header and the archive together -- that is the point.")

nros_assert_archive_pairs_with_header(
    ARCHIVE       "${_NROS_PX4_CPP_A}"
    HEADER        "${NANO_ROS_ROOT}/target/nros-cpp-generated/nros/nros_cpp_config_generated.h"
    SYMBOL_PREFIX "nros_cpp_config_variant_"
    LABEL         "nros_cpp_config_generated.h"
    BUILD_HINT    "${_NROS_PX4_REBUILD_HINT}")

# The C header rides in the same archive: nros-cpp bundles nros-c as an rlib and
# force-links its whole `#[no_mangle]` surface, so `libnros_cpp.a` carries BOTH
# stamps. Measured on a freshly built archive — `nros_config_variant_sz_*` 2
# occurrences, `nros_cpp_config_variant_*` 3. Checking only the C++ one would be
# the issue-0196 shape (coverage narrower than the rule): a PX4 module including
# `<nros/init.h>` gets the C sizes from this header, and they can disagree on
# their own — the C stamp is a SIZE HASH (`sz_<hash>`), not a feature slug, so it
# moves for reasons the C++ slug does not see.
nros_assert_archive_pairs_with_header(
    ARCHIVE       "${_NROS_PX4_CPP_A}"
    HEADER        "${NANO_ROS_ROOT}/target/nros-c-generated/nros/nros_config_generated.h"
    SYMBOL_PREFIX "nros_config_variant_"
    LABEL         "nros_config_generated.h"
    BUILD_HINT    "${_NROS_PX4_REBUILD_HINT}")

# ---------------------------------------------------------------------------
# nros_px4_add_module(MODULE <t> MAIN <m> [BACKENDS <rmw>...] <px4_add_module args>)
# ---------------------------------------------------------------------------
function(nros_px4_add_module)
    # Every px4_add_module keyword must be DECLARED here, even though most are
    # forwarded untouched. cmake_parse_arguments gives a multi-value keyword
    # everything up to the next *declared* keyword — so with only BACKENDS and
    # INCLUDES declared, `BACKENDS uorb SRCS foo.cpp` put SRCS and foo.cpp into
    # BACKENDS, and the generated stub tried to declare
    # `nros_rmw_/abs/path/foo.cpp_register()`. Mirrors px4_add_module.cmake:88-90.
    set(_px4_one   MODULE MAIN STACK_MAIN STACK_MAX PRIORITY)
    set(_px4_multi COMPILE_FLAGS LINK_FLAGS SRCS INCLUDES DEPENDS MODULE_CONFIG)
    set(_px4_opts  EXTERNAL DYNAMIC UNITY_BUILD)

    cmake_parse_arguments(NPX "${_px4_opts}" "${_px4_one}" "BACKENDS;${_px4_multi}" ${ARGN})

    if(NOT NPX_MODULE)
        message(FATAL_ERROR "nros_px4_add_module: MODULE is required")
    endif()
    if(NPX_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nros_px4_add_module: unrecognised arguments: ${NPX_UNPARSED_ARGUMENTS}\n"
            "If px4_add_module gained a keyword, add it to _px4_one/_px4_multi/_px4_opts "
            "in ${CMAKE_CURRENT_FUNCTION_LIST_FILE} — silently forwarding it would let a "
            "multi-value keyword swallow it instead.")
    endif()

    # Rebuild the forwarded argument list, omitting keywords the caller did not
    # pass so px4_add_module never sees a bare keyword with no values.
    set(_fwd "")
    foreach(_k IN LISTS _px4_one _px4_multi)
        if(DEFINED NPX_${_k})
            list(APPEND _fwd ${_k} ${NPX_${_k}})
        endif()
    endforeach()
    foreach(_k IN LISTS _px4_opts)
        if(NPX_${_k})
            list(APPEND _fwd ${_k})
        endif()
    endforeach()
    # nano-ros's own include dirs ride along on INCLUDES.
    list(APPEND _fwd INCLUDES ${_NROS_PX4_INCLUDES})

    # Each named backend contributes its own sources, includes and flags. The
    # alternative — every caller hand-listing the backend's 8 .cpp files, its two
    # include dirs and its one -D, as nros-px4-register-check does — is a copy of
    # build knowledge per module, and the copies drift the moment the backend
    # gains a file. BACKENDS names WHAT; this decides HOW.
    foreach(_b IN LISTS NPX_BACKENDS)
        if(_b STREQUAL "uorb")
            set(_uorb "${NANO_ROS_ROOT}/packages/rmw/uorb/nros-rmw-uorb")
            if(NOT EXISTS "${_uorb}/src/vtable.cpp")
                message(FATAL_ERROR "nros_px4_add_module: uORB backend not found at ${_uorb}")
            endif()
            list(APPEND _fwd
                INCLUDES ${_uorb}/include ${_uorb}/src
                # Flips uorb_abi.hpp to "#include <uORB/uORB.h>" mode, i.e. the
                # real PX4 headers rather than the mock ABI the unit smoke uses.
                COMPILE_FLAGS -DNROS_RMW_UORB_USE_PX4_HEADER=1
                SRCS
                    ${_uorb}/src/vtable.cpp
                    ${_uorb}/src/session.cpp
                    ${_uorb}/src/publisher.cpp
                    ${_uorb}/src/subscriber.cpp
                    ${_uorb}/src/service.cpp
                    ${_uorb}/src/topic_registry.cpp
                    ${_uorb}/src/callback_default.cpp
                    ${_uorb}/src/px4_callback_glue.cpp)
            set(_needs_work_queue TRUE)
        elseif(_b IN_LIST _NROS_PX4_NETWORKED_BACKENDS)
            # A NETWORKED backend contributes nothing at the cmake layer: it is
            # compiled INTO libnros_cpp.a by the cargo feature
            # `rmw-<name>-cffi`, which also pulls in `rmw-cffi` (the seam uORB
            # registers through). All that is needed here is the register call,
            # which the stub below emits from this same list.
            #
            # So "select the outward RMW at build time" (phase-325 W3) is chosen
            # when the ARCHIVE is built, not in cmake:
            #
            #   cargo build -p nros-cpp --no-default-features \
            #       --features std,rmw-zenoh-cffi --release
            #
            # which is the same knob every other example uses, one layer down.
            set(_needs_networked_archive TRUE)
            list(APPEND _networked_backends "${_b}")
        else()
            message(FATAL_ERROR
                "nros_px4_add_module: BACKENDS '${_b}' is not a backend this "
                "helper knows. In-firmware: uorb. Networked: "
                "${_NROS_PX4_NETWORKED_BACKENDS}.")
        endif()
    endforeach()

    px4_add_module(${_fwd})

    if(NOT TARGET ${NPX_MODULE})
        message(FATAL_ERROR "nros_px4_add_module: px4_add_module did not create ${NPX_MODULE}")
    endif()

    # Backend registration hook. nros-c ships a WEAK no-op; a strong definition
    # must override it or the image registers nothing and every entity creation
    # fails at runtime with no backend. In a normal nano-ros build
    # `nano_ros_link_rmw()` generates this TU (cmake/NanoRosLink.cmake) — a
    # hand-rolled PX4 module gets no such generation, so generate the same thing
    # here rather than asking each module to hand-write it.
    #
    # BACKENDS is deliberately required: an EMPTY hook links perfectly and
    # registers nothing, which is the silent-no-op failure this codebase keeps
    # paying for. Better to refuse at configure time.
    if(NOT NPX_BACKENDS)
        message(FATAL_ERROR
            "nros_px4_add_module: BACKENDS is required (e.g. BACKENDS uorb).\n"
            "An empty nros_app_register_backends() links fine and registers NOTHING; "
            "every entity creation then fails at runtime with no backend registered.")
    endif()

    set(_stub_dir "${CMAKE_CURRENT_BINARY_DIR}/_nros_px4/${NPX_MODULE}")
    set(_stub "${_stub_dir}/nros_app_register_backends.c")
    file(MAKE_DIRECTORY "${_stub_dir}")

    string(REPLACE ";" ", " _pretty "${NPX_BACKENDS}")
    set(_c "/* Auto-generated by nros_px4_add_module(). Do not edit.\n")
    string(APPEND _c " * Strong def of nros_app_register_backends() overriding the weak\n")
    string(APPEND _c " * no-op in nros-c. Backends registered: ${_pretty}\n */\n")
    foreach(_b IN LISTS NPX_BACKENDS)
        string(APPEND _c "extern int nros_rmw_${_b}_register(void);\n")
    endforeach()
    string(APPEND _c "void nros_app_register_backends(void) {\n")
    foreach(_b IN LISTS NPX_BACKENDS)
        string(APPEND _c "    (void)nros_rmw_${_b}_register();\n")
    endforeach()
    string(APPEND _c "}\n")
    file(WRITE "${_stub}" "${_c}")

    # Issue 1050 defect (3) — `BACKENDS` also decides which backend
    # `nros::init()` OPENS, not only which ones get registered.
    #
    # The stub above is the image's declaration, and until the selector rung
    # existed the runtime could not read it: the registry's default was slot 0,
    # i.e. whichever backend registered first, and on hosted POSIX a Rust backend
    # inside `libnros_cpp.a` registers from an `.init_array` ctor BEFORE `main` —
    # so it beats the stub every time. The precheck above refuses that archive,
    # which closes the accident; this closes the API gap, so a module that
    # deliberately links two backends still opens the one it named first.
    #
    # Only for a SINGLE declared backend. With several there is no single answer,
    # and inventing one from list order would be the same "whoever is first"
    # policy this issue is about. Such a module names its sessions explicitly
    # (`nros::init_with_rmw`, or `MultiExecutor`).
    list(LENGTH NPX_BACKENDS _npx_backend_count)
    if(_npx_backend_count EQUAL 1)
        list(GET NPX_BACKENDS 0 _npx_primary)
        target_compile_definitions(${NPX_MODULE} PRIVATE
            "NROS_ENTRY_RMW=\"${_npx_primary}\"")
    endif()

    # The stub goes INTO the module archive, and the linker is told up front that
    # it needs the symbol. Three shapes were measured before this one:
    #
    #   target_sources() alone      -> `undefined reference to
    #       nros_app_register_backends`. A linker pulls an archive member only if
    #       it resolves an undefined symbol AT THE MOMENT it scans that archive,
    #       and PX4 puts the module archive BEFORE libnros_cpp.a. By the time
    #       libnros_cpp.a asks for the hook, the module archive is behind us.
    #
    #   an OBJECT library           -> its object never reaches the link line at
    #       all: PX4 assembles the px4 executable from module ARCHIVES it collects
    #       itself, not from CMake's object propagation.
    #
    #   a separate trailing archive -> resolved the hook, then failed on
    #       `nros_rmw_uorb_register`. The dependency is genuinely CIRCULAR —
    #       module.a needs libnros_cpp.a, which needs the hook, which needs the
    #       backend back in module.a — so no single ordering of distinct archives
    #       can satisfy it. (CMake 3.24's $<LINK_GROUP:RESCAN> would express the
    #       --start-group/--end-group answer; this tree is on 3.22.)
    #
    # `-u <sym>` makes the symbol undefined from the START, so the member is
    # pulled on the FIRST scan of the module archive, before anything asks for it.
    # Same class as nros-c's FORCE_LINK anchors (CLAUDE.md): a symbol nothing has
    # referenced yet is a symbol the linker feels free to drop.
    target_sources(${NPX_MODULE} PRIVATE "${_stub}")
    target_link_options(${NPX_MODULE} PUBLIC "-Wl,--undefined=nros_app_register_backends")

    # A networked backend lives inside libnros_cpp.a, so the archive must have
    # been built with the matching feature. Nothing else checks this: a mismatched
    # archive links every OTHER symbol fine and dies only on
    # nros_rmw_<name>_register, at the very end of a ~10-minute PX4 build.
    #
    # The check MUST use the rust toolchain's llvm-nm. The system nm (binutils +
    # LLVM 14 gold plugin) cannot parse rust-1.96/LLVM 22 bitcode members, and it
    # does not fail cleanly — it reads the FEW non-bitcode members and reports
    # their symbols, so it saw 18 `nros_` symbols while missing
    # nros_rmw_zenoh_register entirely. A first draft tried to fail-open on "nm
    # saw nothing"; a partial read defeats that, and the guard confidently
    # rejected a perfectly good archive. A guard using the wrong tool is worse
    # than no guard: it is wrong with authority.
    #
    # So: locate llvm-nm through rustc's own sysroot, and if it is not there,
    # SKIP the check rather than guess. The link error remains the backstop.
    #
    # Issue 1050 — the check runs for EVERY module, not only for one that named a
    # networked backend, and it checks BOTH directions.
    #
    # It used to sit behind `if(_networked_backends)`, which is exactly the set of
    # modules the bug cannot happen to. The uORB demo declares `BACKENDS uorb`, so
    # the whole block was skipped for it — and a uORB-only module linking a
    # zenoh-carrying archive is the failure: on hosted POSIX the backend's
    # `.init_array` ctor registers zenoh BEFORE main, hence before the generated
    # `nros_app_register_backends()`, zenoh takes slot 0, `nros::init()` opens the
    # default slot, dials tcp/127.0.0.1:7447, finds nothing, and the module dies
    # with `nros::init() failed` -> PX4_ERROR -> shell status 255.
    #
    # So `BACKENDS` did not mean what it reads as. It read "this image registers
    # uorb"; what it named was "this image ALSO registers uorb". The absence half
    # below is what makes the keyword an assertion instead of a hint, and it is
    # the same defect shape as issue 1046 one layer up: a guard whose predicate
    # cannot observe the case its own message describes.
    #
    # `just px4 build-sitl-example` now builds the archive it links, which fixes
    # the RECIPE. This fixes the SEAM: a hand-run
    # `make px4_sitl_default EXTERNAL_MODULES_LOCATION=...` against an archive
    # someone else's recipe left behind is caught here, at configure time,
    # instead of at `nros_uorb_demo start`.
    #
    # Cost: one extra `llvm-nm --defined-only` per configure for a uORB-only
    # module. Measured 0.012 s on the 26 MB archive. It hashes nothing and
    # watches nothing, so it re-stales nothing (phase-424's constraint).
    execute_process(COMMAND rustc --print sysroot
        OUTPUT_VARIABLE _rustc_sysroot OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET RESULT_VARIABLE _sysroot_rc)
    set(_llvm_nm "")
    if(_sysroot_rc EQUAL 0)
        file(GLOB _llvm_nm_candidates
            "${_rustc_sysroot}/lib/rustlib/*/bin/llvm-nm")
        if(_llvm_nm_candidates)
            list(GET _llvm_nm_candidates 0 _llvm_nm)
        endif()
    endif()

    if(_llvm_nm)
        execute_process(COMMAND "${_llvm_nm}" --defined-only "${_NROS_PX4_CPP_A}"
            OUTPUT_VARIABLE _nm_out ERROR_QUIET)

        # (a) every DECLARED networked backend must be in the archive, or the
        #     link dies on nros_rmw_<name>_register ~10 minutes from now.
        foreach(_nb IN LISTS _networked_backends)
            _nros_px4_archive_defines_backend(_has "${_nm_out}" "${_nb}")
            if(NOT _has)
                message(FATAL_ERROR
                    "nros_px4_add_module: BACKENDS lists '${_nb}', but\n"
                    "    ${_NROS_PX4_CPP_A}\n"
                    "does not define nros_rmw_${_nb}_register. Rebuild it "
                    "with that backend:\n"
                    "    cargo build -p nros-cpp --no-default-features "
                    "--features std,rmw-${_nb}-cffi --release")
            endif()
        endforeach()

        # (b) and no UNDECLARED one may be, because it registers itself first.
        foreach(_nb IN LISTS _NROS_PX4_NETWORKED_BACKENDS)
            if(_nb IN_LIST _networked_backends)
                continue()
            endif()
            _nros_px4_archive_defines_backend(_has "${_nm_out}" "${_nb}")
            if(_has)
                string(REPLACE ";" " " _declared "${NPX_BACKENDS}")
                message(FATAL_ERROR
                    "nros_px4_add_module: ${NPX_MODULE} declares BACKENDS "
                    "'${_declared}', but\n"
                    "    ${_NROS_PX4_CPP_A}\n"
                    "defines nros_rmw_${_nb}_register — a backend this module did "
                    "not ask for.\n"
                    "That archive was left by another build (e.g. `just px4 "
                    "build-bridge-example`). On hosted POSIX the '${_nb}' backend "
                    "registers itself from an .init_array ctor BEFORE main, so it "
                    "takes RMW slot 0 ahead of this module's generated "
                    "nros_app_register_backends(), nros::init() opens IT, and the "
                    "module fails at startup with `nros::init() failed` (issue "
                    "1050).\n"
                    "Rebuild the archive for THIS module:\n"
                    "    cargo build -p nros-cpp --no-default-features "
                    "--features std,rmw-cffi,platform-posix --release\n"
                    "or, from the repo root, just:\n"
                    "    just px4 build-sitl-example\n"
                    "To link a different archive on purpose, point at it with "
                    "-DNROS_CPP_ARCHIVE=<path> / NROS_CPP_ARCHIVE=<path>.")
            endif()
        endforeach()
    else()
        message(STATUS
            "nros_px4_add_module: llvm-nm not found via rustc sysroot; "
            "skipping the backend-symbol precheck (the link will still catch "
            "a mismatched archive, ~10 min later; an UNDECLARED backend in the "
            "archive links fine and fails at runtime — issue 1050).")
    endif()

    # PUBLIC so the archives propagate to the final px4 link. px4_add_module
    # produces a STATIC library that the px4 executable links; a PRIVATE link here
    # would satisfy nothing at that final line.
    set(_nros_px4_link_archives "${_NROS_PX4_CPP_A}" "${_NROS_PX4_PLATFORM_A}")

    # Issue 0436 — zenoh-pico has ONE source of truth: `libnros_cpp.a`.
    #
    # This used to link a SECOND archive (`libnros_rmw_zenoh_staticlib.a`) for
    # zenoh-pico's platform layer (z_clock_*, _z_condvar_*, _z_task_*, the socket
    # shims). That was true when the umbrella was built WITHOUT a platform
    # feature: its bundled zenoh-pico had the 1465 core symbols and none of the
    # 111 platform ones, so the platform archive supplied them.
    #
    # But the platform archive is a COMPLETE zenoh-pico (1576 symbols), so the
    # image then carried TWO copies of the core — both defining `_z_open_socket`.
    # One definition wins per symbol while each copy keeps its own `static` state,
    # and a session opened against one copy's state made socket calls that
    # resolved into the other's: `z_open` failed with CONNECTION_FAILED having put
    # nothing on the wire. That is the duplicate-symbol class the project bans
    # outright (`check-no-allow-multiple-def`), one layer below the archives it
    # usually catches.
    #
    # The fix is a single source, not a second archive: build the umbrella with a
    # PLATFORM feature and its zenoh-pico is complete —
    #
    #   cargo build -p nros-cpp --no-default-features \
    #       --features std,rmw-zenoh-cffi,platform-posix --release
    #
    # verified to yield all 1576 symbols, i.e. exactly what the separate archive
    # had. So nothing extra is linked here; a missing platform layer now surfaces
    # as an undefined-symbol link error naming `z_clock_now` & friends, whose fix
    # is the feature above rather than another archive.
    if("zenoh" IN_LIST _networked_backends)
        foreach(_stale IN ITEMS "${NROS_ZENOH_ARCHIVE}" "$ENV{NROS_ZENOH_ARCHIVE}")
            if(NOT "${_stale}" STREQUAL "")
                message(WARNING
                    "nros_px4_add_module: NROS_ZENOH_ARCHIVE is set (${_stale}) but is "
                    "no longer linked — zenoh-pico now comes solely from libnros_cpp.a "
                    "(issue 0436). Linking both put two copies of zenoh-pico in one "
                    "image. Build the umbrella with a platform feature instead.")
            endif()
        endforeach()
    endif()

    target_link_libraries(${NPX_MODULE} PUBLIC ${_nros_px4_link_archives})

    # px4_work_queue provides SubscriptionCallbackWorkItem + WorkQueueManager,
    # which the uORB backend's push-wake glue needs. External modules cannot list
    # it in DEPENDS (the target does not exist when PX4 walks
    # EXTERNAL_MODULES_LOCATION), so wire it post-hoc — add_dependencies is
    # order-agnostic and the symbols resolve at the final px4 link.
    if(_needs_work_queue AND TARGET px4_work_queue)
        add_dependencies(${NPX_MODULE} px4_work_queue)
    endif()

    # nros-cpp is C++ and pulls libstdc++ symbols; make sure the C++ driver is
    # used even if this module's own sources are C.
    set_target_properties(${NPX_MODULE} PROPERTIES LINKER_LANGUAGE CXX)
endfunction()
