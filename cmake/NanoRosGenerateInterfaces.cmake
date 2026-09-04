#[=======================================================================[.rst:
NanoRosGenerateInterfaces
-------------------------

**Single source of truth.** ``<repo-root>/cmake/NanoRosGenerateInterfaces.cmake``
is the one canonical copy — the root entry CMake (POSIX) and every cross-compile
platform module (``cmake/platform/nano-ros-{freertos,threadx,nuttx}.cmake``)
``include()`` it. The former second copy in the ``packages/codegen`` submodule
(``nros-codegen-c/cmake/``) was deleted with the ``nros-codegen-c`` crate in
Phase 195.D — there is no copy to mirror into any more.

Generate C or C++ bindings for ROS 2 interface files (.msg, .srv, .action).

This module provides two functions:

``nros_find_interfaces()``
  High-level: reads ``package.xml``, resolves transitive interface
  dependencies via AMENT index (with bundled fallback), and generates
  bindings for all required packages.

  .. code-block:: cmake

    nros_find_interfaces([LANGUAGE C|CPP] [SKIP_INSTALL])

``nros_generate_interfaces()``
  Low-level: generates bindings for a single package.

  .. code-block:: cmake

    nros_generate_interfaces(<target>
      [<interface_files>...]
      [LANGUAGE C|CPP]
      [DEPENDENCIES <packages>...]
      [SKIP_INSTALL]
    )

Prerequisites:
  Reached automatically when the user's ``CMakeLists.txt`` calls
  ``add_subdirectory(nano-ros)``. The root entry CMake `include()`s
  this module and points ``_NANO_ROS_CODEGEN_TOOL`` at the
  Corrosion-built ``nros-codegen`` target. No install step needed.

#]=======================================================================]

# Allow callers to override _NANO_ROS_PREFIX (e.g. for in-tree cross-compilation
# where the codegen cmake lives under packages/ but the prefix is the project root).
if(NOT DEFINED _NANO_ROS_PREFIX AND NOT DEFINED CACHE{_NANO_ROS_PREFIX})
    get_filename_component(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)
endif()
# Phase 144 — same scope concern as _NANO_ROS_CMAKE_DIR below.
if(DEFINED _NANO_ROS_PREFIX AND NOT DEFINED CACHE{_NANO_ROS_PREFIX})
    set(_NANO_ROS_PREFIX "${_NANO_ROS_PREFIX}" CACHE INTERNAL
        "Effective nano-ros source/install prefix used by codegen")
endif()
# Phase 144 — cache as INTERNAL so `nros_generate_interfaces` reaches
# the right path when invoked from a sibling subdir scope (e.g. an
# example tree pulling nano-ros via add_subdirectory(<repo-root>)).
# Plain `set()` at include time only lands in the including scope; CPP
# codegen calls configure_file() with this path from the call-site
# scope and saw the empty value before this cache promotion.
set(_NANO_ROS_CMAKE_DIR "${CMAKE_CURRENT_LIST_DIR}" CACHE INTERNAL
    "Directory containing NanoRosGenerateInterfaces.cmake's template files")

# Phase 246 — shared codegen helpers (lib.rs assembly, rs-closure collect/export)
# used by both this generator and the Zephyr-module sibling. include_guard'd.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCodegenCore.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosBoardFacts.cmake")
# `nros_read_package_xml_body()` — regex reads of a package.xml must not see
# commented-out elements (phase-348 W1).
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosPackageXml.cmake")

# =========================================================================
# Locate the nros-codegen tool
# =========================================================================

# phase-336 — the profile and its target directory come from the shared table
# (`nros profile`), so this file no longer carries a default literal or its own
# `dev → debug` special case.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCargoProfile.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRosEdition.cmake")
nros_resolve_cargo_profile()
set(NROS_CODEGEN_CARGO_PROFILE "${NROS_CARGO_PROFILE}" CACHE STRING
    "Cargo profile whose target directory is searched for nros-codegen" FORCE)
set(_NROS_CODEGEN_TARGET_PROFILE_DIR "${NROS_CARGO_PROFILE_DIR}")

# Phase 218: the `nros` CLI lives in-tree at packages/cli/ (built by
# `just setup-cli`; `source ./activate.sh` puts it on PATH). Cross-compile
# platform modules pre-set `_NANO_ROS_CODEGEN_TOOL` via nros_bootstrap_codegen();
# a consumer may override with -D_NANO_ROS_CODEGEN_TOOL=<path>. Shared find/
# validate/cache lives in the core (Phase 246.2b).
_nros_resolve_codegen_tool(_NANO_ROS_CODEGEN_TOOL)

# _nros_resolve_interface(<target> <relpath> <out_var>) — thin wrapper over the
# shared core resolver (Phase 246.2b), supplying the bundled-interface prefix.
function(_nros_resolve_interface target relpath out_var)
  _nros_resolve_interface_file("${target}" "${relpath}" _r
    BUNDLED_PREFIX "${_NANO_ROS_PREFIX}")
  set(${out_var} "${_r}" PARENT_SCOPE)
endfunction()

# =========================================================================
# nros_generate_interfaces(<target> <files>...
#     [DEPENDENCIES <deps>...] [SKIP_INSTALL])
#
# **Phase 210.E.4 — DEPRECATED for new code.** Prefer the upstream-shape
# entry points: `rosidl_generate_interfaces(<target> <files>...)` from a
# msg pkg's CMakeLists.txt (Phase 210.A.1), or `find_package(<pkg>)`
# from a consumer's CMakeLists.txt (Phase 210.A.2). Both route through
# this function under the hood; calling it directly is supported for
# back-compat but bypasses the ROS-convention surface.
# =========================================================================
function(nros_generate_interfaces target)
  # NO_FFI_CRATE — DEPRECATED no-op (phase-306 W1, issue 0253): the split
  # types/exports codegen makes every package's FFI crate export only its own
  # `nros_cpp_*` symbols, so the superset-archive suppression this flag drove
  # is gone. The keyword stays accepted so existing callers don't break.
  cmake_parse_arguments(_ARG
    "SKIP_INSTALL;NO_FFI_CRATE"
    "ROS_EDITION;LANGUAGE;CODEGEN_CONFIG"
    "DEPENDENCIES"
    ${ARGN}
  )
  if(_ARG_NO_FFI_CRATE)
    message(STATUS
      "nros_generate_interfaces(${target}): NO_FFI_CRATE is deprecated and "
      "ignored (per-package FFI crates export only their own symbols since "
      "phase-306).")
  endif()

  # phase-304 W2b (RFC-0056) — an explicit ROS_EDITION wins; otherwise inherit
  # the workspace edition baked by `nano_ros_workspace(EDITION …)`
  # (`NANO_ROS_ROS_EDITION`) so the codegen type-hash matches the runtime
  # `ros-<edition>` keyexpr feature. Neither set → humble (byte-identical).
  _nros_resolve_ros_edition("${_ARG_ROS_EDITION}" _ARG_ROS_EDITION)

  if(NOT DEFINED _ARG_LANGUAGE OR _ARG_LANGUAGE STREQUAL "")
    set(_ARG_LANGUAGE "C")
  endif()
  string(TOUPPER "${_ARG_LANGUAGE}" _ARG_LANGUAGE)

  # Phase 219.H — idempotency guard.
  #
  # Two sibling Node pkgs that both `<depend>` on the same interface pkg
  # (e.g. each calls `nros_find_interfaces(LANGUAGE CPP)` with
  # `std_msgs` in `package.xml`) reach this fn twice for the same
  # `(target, language)` pair. Without a guard the second call dies on
  # the `add_library(${target}__nano_ros_${_lang_flag})` collision
  # (line ~462 / ~665 / ~673) or the `add_custom_target` companion
  # (line ~471 / ~607).
  #
  # The existing per-builtin guards (lines ~282-290) only cover the
  # `builtin_interfaces` / `unique_identifier_msgs` / `action_msgs`
  # auto-deps; every other interface pkg collides. Generalise the
  # `if(NOT TARGET …)` discipline by short-circuiting the whole body
  # once the target has been generated.
  #
  # Closes Phase 219 workflow-review Gap 3.
  if(_ARG_LANGUAGE STREQUAL "CPP")
    set(_idempotency_check_target "${target}__nano_ros_cpp")
  else()
    set(_idempotency_check_target "${target}__nano_ros_c")
  endif()
  if(TARGET ${_idempotency_check_target})
    # phase-263 A4 — the interface lib was already generated by an EARLIER sibling
    # pkg (a different `add_subdirectory` scope), so this consumer generates nothing
    # new. But `nano_ros_node_register` auto-links interface libs from THIS directory's
    # `NROS_GENERATED_INTERFACE_LIBS` property (set in the generating path below) — a
    # sibling that depends ONLY on an already-generated interface would otherwise get an
    # EMPTY property and fail to compile (`example_interfaces.h: No such file`). This was
    # latent: every prior consumer (e.g. `c_add_client_pkg`) also generated at least one
    # FRESH interface (std_msgs), so its component lib picked up the shared interface
    # target transitively; a pure-consumer-of-an-already-generated-interface pkg
    # (`c_fib_server_pkg` / `c_fib_client_pkg`) is the first to hit it. Register the
    # existing target in this directory's scope so the auto-link still finds it. The
    # Zephyr guard mirrors the generating path (the FFI is whole-archived into `app`
    # there, not a linkable `<pkg>__nano_ros_*` lib).
    if(NOT NANO_ROS_PLATFORM STREQUAL "zephyr")
      set_property(DIRECTORY APPEND PROPERTY
        NROS_GENERATED_INTERFACE_LIBS "${_idempotency_check_target}")
    endif()
    return()
  endif()

  # Resolve or auto-discover interface files
  set(_interface_files "")

  if(_ARG_UNPARSED_ARGUMENTS)
    # Explicit files: resolve each via local + ament + bundled
    foreach(_relpath ${_ARG_UNPARSED_ARGUMENTS})
      _nros_resolve_interface("${target}" "${_relpath}" _abs_path)
      if(_abs_path STREQUAL "NOTFOUND")
        message(FATAL_ERROR
          "nros_generate_interfaces(): cannot find '${_relpath}' for "
          "package '${target}'.\n"
          "  Searched:\n"
          "    ${CMAKE_CURRENT_SOURCE_DIR}/${_relpath}\n"
          "    AMENT_PREFIX_PATH/share/${target}/${_relpath}\n"
          "    ${_NANO_ROS_PREFIX}/share/nano-ros/interfaces/${target}/${_relpath}\n"
          "    ${_NANO_ROS_PREFIX}/packages/cli/interfaces/${target}/${_relpath}\n"
          "  Hint: check the file path or set AMENT_PREFIX_PATH.")
      endif()
      list(APPEND _interface_files "${_abs_path}")
    endforeach()
  else()
    # Auto-discover: no files specified — search local dirs, ament, bundled
    # phase-360 W2 — CONFIGURE_DEPENDS on every glob. A plain `file(GLOB)`
    # runs at CONFIGURE time only, so ADDING an interface file left the
    # build generating the old set until something unrelated forced a
    # reconfigure. The generated sources are a function of this list, so a
    # stale list is a museum artifact — the class phase-360 exists for. The
    # tree already used CONFIGURE_DEPENDS in 26 other places; these were
    # missed.
    # 1. Local directories
    file(GLOB _local_msg CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/msg/*.msg")
    file(GLOB _local_srv CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/srv/*.srv")
    file(GLOB _local_action CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/action/*.action")
    list(APPEND _interface_files ${_local_msg} ${_local_srv} ${_local_action})

    # 2. Ament index
    if(NOT _interface_files AND DEFINED ENV{AMENT_PREFIX_PATH})
      string(REPLACE ":" ";" _ament_paths "$ENV{AMENT_PREFIX_PATH}")
      foreach(_prefix ${_ament_paths})
        file(GLOB _ament_msg CONFIGURE_DEPENDS "${_prefix}/share/${target}/msg/*.msg")
        file(GLOB _ament_srv CONFIGURE_DEPENDS "${_prefix}/share/${target}/srv/*.srv")
        file(GLOB _ament_action CONFIGURE_DEPENDS "${_prefix}/share/${target}/action/*.action")
        list(APPEND _interface_files ${_ament_msg} ${_ament_srv} ${_ament_action})
      endforeach()
    endif()

    # 3. Bundled interfaces
    if(NOT _interface_files)
      file(GLOB _bundled_msg CONFIGURE_DEPENDS "${_NANO_ROS_PREFIX}/share/nano-ros/interfaces/${target}/msg/*.msg")
      file(GLOB _bundled_srv CONFIGURE_DEPENDS "${_NANO_ROS_PREFIX}/share/nano-ros/interfaces/${target}/srv/*.srv")
      file(GLOB _bundled_action CONFIGURE_DEPENDS "${_NANO_ROS_PREFIX}/share/nano-ros/interfaces/${target}/action/*.action")
      list(APPEND _interface_files ${_bundled_msg} ${_bundled_srv} ${_bundled_action})
    endif()

    # 4. Source tree — issue 0663. Same fact as tier 3, different layout: the
    # bundled path is where an INSTALL puts these files, `packages/cli/interfaces`
    # is where the checkout ships them. Without this a ROS-less source tree
    # found no `std_msgs` at all, and the error listed three paths none of which
    # was the one holding the file.
    if(NOT _interface_files)
      file(GLOB _src_msg CONFIGURE_DEPENDS "${_NANO_ROS_PREFIX}/packages/cli/interfaces/${target}/msg/*.msg")
      file(GLOB _src_srv CONFIGURE_DEPENDS "${_NANO_ROS_PREFIX}/packages/cli/interfaces/${target}/srv/*.srv")
      file(GLOB _src_action CONFIGURE_DEPENDS "${_NANO_ROS_PREFIX}/packages/cli/interfaces/${target}/action/*.action")
      list(APPEND _interface_files ${_src_msg} ${_src_srv} ${_src_action})
    endif()

    if(NOT _interface_files)
      message(FATAL_ERROR
        "nros_generate_interfaces(): no interface files found for '${target}'.\n"
        "  Searched:\n"
        "    ${CMAKE_CURRENT_SOURCE_DIR}/{msg,srv,action}/\n"
        "    AMENT_PREFIX_PATH/share/${target}/{msg,srv,action}/\n"
        "    ${_NANO_ROS_PREFIX}/share/nano-ros/interfaces/${target}/{msg,srv,action}/\n"
        "  Hint: add msg/*.msg locally or source ROS 2 setup.bash to populate AMENT_PREFIX_PATH.")
    endif()
  endif()

  # Output directory — language-specific subdirectory
  if(_ARG_LANGUAGE STREQUAL "CPP")
    set(_subdir "nano_ros_cpp")
    set(_lang_flag "cpp")
  else()
    set(_subdir "nano_ros_c")
    set(_lang_flag "c")
  endif()

  # ---- Action auto-closure (177.32) ----
  # A package that carries a `.action` uses the shared action_msgs types at
  # runtime: the cancel service (action_msgs/CancelGoal) and the status topic
  # (action_msgs/GoalStatusArray), which in turn pull unique_identifier_msgs
  # (UUID). The Cyclone backend resolves those as *real* IDL descriptors (the
  # status-publish bridge reads the real GoalStatusArray_ op layout; the cancel
  # service uses the plain real-CancelGoal_ path), so the consuming package must
  # generate + depend on action_msgs (+ its UUID dep) — otherwise the descriptor
  # ts-lib doesn't whole-archive in and the cpp FFI glue can't close over the
  # types. Do it automatically so action examples need NO per-package wiring.
  # Guarded by target existence (idempotent across packages) and skipped when
  # generating action_msgs / unique_identifier_msgs themselves (no recursion).
  set(_nros_has_action FALSE)
  foreach(_if ${_interface_files})
    if(_if MATCHES "\\.action$")
      set(_nros_has_action TRUE)
    endif()
  endforeach()
  if(_nros_has_action
     AND NOT target STREQUAL "action_msgs"
     AND NOT target STREQUAL "unique_identifier_msgs"
     AND NOT target STREQUAL "builtin_interfaces")
    # action_msgs/GoalInfo pulls BOTH unique_identifier_msgs/UUID AND
    # builtin_interfaces/Time, so the closure must generate both (it declares
    # `DEPENDENCIES builtin_interfaces` below, but a declared dep only wires the
    # FFI-glue include — the dep package's own headers still need generating, or
    # `action_msgs_msg_goal_info.h`'s `#include "builtin_interfaces/...time.h"`
    # resolves nowhere). Pre-177.32-fix this was missed (only UUID generated).
    if(NOT TARGET builtin_interfaces__nano_ros_${_lang_flag})
      nros_generate_interfaces(builtin_interfaces
        LANGUAGE ${_ARG_LANGUAGE} SKIP_INSTALL)
    endif()
    if(NOT TARGET unique_identifier_msgs__nano_ros_${_lang_flag})
      nros_generate_interfaces(unique_identifier_msgs
        LANGUAGE ${_ARG_LANGUAGE} SKIP_INSTALL)
    endif()
    if(NOT TARGET action_msgs__nano_ros_${_lang_flag})
      nros_generate_interfaces(action_msgs
        DEPENDENCIES builtin_interfaces unique_identifier_msgs
        LANGUAGE ${_ARG_LANGUAGE} SKIP_INSTALL)
    endif()
    if(NOT "action_msgs" IN_LIST _ARG_DEPENDENCIES)
      list(APPEND _ARG_DEPENDENCIES "action_msgs")
    endif()
  endif()

  # Phase 123.A.7 — workspace-shared codegen cache.
  # When NANO_ROS_GEN_CACHE_DIR is set (cmake var or env var), all
  # packages emit codegen into the same shared dir keyed by
  # (language, target). CMake's mtime-based add_custom_command
  # dependency tracking means the second package to configure sees
  # the up-to-date output files and skips the regeneration.
  #
  # Multi-package workspace win: `std_msgs` codegen runs once across
  # the workspace instead of once per consuming package.
  #
  # Concurrency caveat: colcon's --parallel-workers can race two
  # packages on the same codegen target. Mitigation: declare an
  # explicit dependency between packages in package.xml so colcon
  # serializes them. Documented in installation.md (A.9).
  set(_gen_cache_root "")
  if(DEFINED NANO_ROS_GEN_CACHE_DIR AND NOT NANO_ROS_GEN_CACHE_DIR STREQUAL "")
    set(_gen_cache_root "${NANO_ROS_GEN_CACHE_DIR}")
  elseif(DEFINED ENV{NANO_ROS_GEN_CACHE_DIR} AND NOT "$ENV{NANO_ROS_GEN_CACHE_DIR}" STREQUAL "")
    set(_gen_cache_root "$ENV{NANO_ROS_GEN_CACHE_DIR}")
  endif()

  if(_gen_cache_root)
    set(_umbrella_dir "${_gen_cache_root}/${_subdir}")
  else()
    set(_umbrella_dir "${CMAKE_CURRENT_BINARY_DIR}/${_subdir}")
  endif()
  set(_output_dir "${_umbrella_dir}/${target}")
  file(MAKE_DIRECTORY "${_output_dir}")
  file(MAKE_DIRECTORY "${_output_dir}/msg")
  file(MAKE_DIRECTORY "${_output_dir}/srv")
  file(MAKE_DIRECTORY "${_output_dir}/action")

  # ---- Build JSON arguments file ----
  # Phase 123.A.7 — when the cache is active, store the args file in
  # the cache too so the content-compare mtime preservation works
  # across packages (otherwise each package writes a fresh args file
  # into its own build dir and triggers regeneration).
  if(_gen_cache_root)
    set(_args_file "${_gen_cache_root}/nano_ros_generate_${_lang_flag}_args__${target}.json")
  else()
    set(_args_file "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_generate_${_lang_flag}_args__${target}.json")
  endif()

  # Build + write the codegen args JSON (shared core, Phase 246.2 — was an
  # identical copy in the Zephyr generator).
  _nros_write_codegen_args_json(
    ARGS_FILE "${_args_file}"
    PACKAGE "${target}"
    OUTPUT_DIR "${_output_dir}"
    ROS_EDITION "${_ARG_ROS_EDITION}"
    CODEGEN_CONFIG "${_ARG_CODEGEN_CONFIG}"
    INTERFACE_FILES ${_interface_files}
    DEPS ${_ARG_DEPENDENCIES})

  # Predict the files codegen will emit — feeds add_custom_command OUTPUT below
  # (shared core, Phase 246.2).
  _nros_predict_generated_outputs(_generated_headers _generated_sources _generated_rs_files
    LANGUAGE "${_ARG_LANGUAGE}"
    PACKAGE "${target}"
    OUTPUT_DIR "${_output_dir}"
    INTERFACE_FILES ${_interface_files})

  # ---- Derived size bounds (phase-403 W6) ----
  # Codegen writes these unconditionally, for every language and even for a
  # package with no message types, so they are safe as declared OUTPUTs. They
  # are what W4 (exact zenoh payload classes) and W5 (arena from the entities
  # actually registered) need, and what a `.conf` author currently produces by
  # reading a generated header by eye. See nros_message_bounds_files().
  nros_message_bounds_files("${_output_dir}" _bounds_json _bounds_cmake)
  set(${target}_MESSAGE_BOUNDS_JSON "${_bounds_json}" CACHE INTERNAL
      "phase-403 W6: derived per-type serialized-size bounds for ${target}")
  set(${target}_MESSAGE_BOUNDS_CMAKE "${_bounds_cmake}" CACHE INTERNAL
      "phase-403 W6: include() this to read ${target}'s bounds as CMake variables")
  # phase-403 W8 (issue 0940) -- and now something READS them. One image-wide
  # list, so `nros_find_interfaces()` composes the closure without knowing which
  # generator produced each fragment.
  #
  # This lane emits the fragment as a BUILD-time custom-command output, so on a
  # clean tree it does not exist yet at configure time and the derivation
  # refuses (loudly, naming the missing file's package). It is present from the
  # next configure on. The Zephyr lane runs codegen AT configure time and has it
  # immediately -- the difference is the lane's, not this wave's.
  nros_message_bounds_register_fragment("${_bounds_cmake}")

  # ---- Custom command ----
  add_custom_command(
    OUTPUT ${_generated_headers} ${_generated_sources} ${_generated_rs_files}
           "${_bounds_json}" "${_bounds_cmake}"
    COMMAND "${_NANO_ROS_CODEGEN_TOOL}" codegen --language "${_lang_flag}" --args-file "${_args_file}"
    DEPENDS ${_interface_files} "${_args_file}" "${_NANO_ROS_CODEGEN_TOOL}"
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    COMMENT "Generating nros ${_ARG_LANGUAGE} interfaces for ${target}"
    VERBATIM
  )

  # ---- Library target ----
  if(_ARG_LANGUAGE STREQUAL "CPP")
    # C++ target: header-only INTERFACE + Rust FFI staticlib for message bindings
    set(_lib_target "${target}__nano_ros_cpp")
    add_library(${_lib_target} INTERFACE)
    target_include_directories(${_lib_target}
      INTERFACE
        $<BUILD_INTERFACE:${_output_dir}>
        $<BUILD_INTERFACE:${_umbrella_dir}>
        $<INSTALL_INTERFACE:include/${target}>
    )

    # Custom target to drive codegen (INTERFACE libraries don't trigger custom commands)
    add_custom_target(${_lib_target}_gen DEPENDS ${_generated_headers} ${_generated_rs_files})
    add_dependencies(${_lib_target} ${_lib_target}_gen)

    # ---- Build Rust FFI glue for generated message types ----
    # The generated .rs files provide extern "C" publish/serialize/deserialize
    # functions. We compile them into a static library via cargo.
    #
    # phase-306 W1 (issue 0253): every package builds its OWN crate. The crate
    # includes dependency `*_types.rs` (crate-mangled, duplicate-safe) but only
    # its OWN `*_exports.rs` (`_nros_collect_rs_closure` filters dep exports),
    # so each archive defines each `nros_cpp_*` symbol exactly once and any
    # combination of interface archives links.
    if(_generated_rs_files)
      # Phase 123.A.7 — share the FFI crate build dir across packages
      # when NANO_ROS_GEN_CACHE_DIR is set.
      if(_gen_cache_root)
        set(_ffi_crate_dir "${_gen_cache_root}/nano_ros_cpp_ffi_${target}")
      else()
        set(_ffi_crate_dir "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp_ffi_${target}")
      endif()
      set(_ffi_crate_src "${_ffi_crate_dir}/src")
      set(_ffi_target_dir "${_ffi_crate_dir}/target")
      # Phase 144 — resolve nros-serdes path against either the install
      # layout (`<prefix>/share/nano-ros/rust/nros-serdes`) or the
      # in-tree source layout (`<repo-root>/packages/core/nros-serdes`).
      # `add_subdirectory(<repo-root>)` consumers see the source path;
      # installed `find_package(NanoRos)` consumers see the share path.
      set(_serdes_dir "${_NANO_ROS_PREFIX}/share/nano-ros/rust/nros-serdes")
      if(NOT EXISTS "${_serdes_dir}/Cargo.toml")
          set(_serdes_dir "${_NANO_ROS_PREFIX}/packages/core/nros-serdes")
      endif()

      # phase-336 — the artifact lands under the profile's directory, not a
      # hardcoded `release/`. This pair of literals was the reason a non-release
      # build could configure fine and then fail to find its own staticlib.
      #
      # The glue uses the `cpp-ffi-glue` CARVE-OUT rather than the ambient
      # profile: it is a second Rust staticlib in a link that already contains
      # libnros_cpp.a, and at `lto = "off"` both carry std's panicking codegen
      # unit → `multiple definition of __rustc::rust_begin_unwind`. Fat LTO
      # internalizes it. (This is why the generated manifest pinned `lto = true`
      # while the profile was hardcoded `release`.)
      nros_resolve_carve_out_profile(cpp-ffi-glue _NROS_FFI)
      # phase-340 W3 — ONE spelling of `--target`, host included.
      #
      # This used to branch: `--target <triple>` when `Rust_CARGO_TARGET` was
      # visible, and NO `--target` otherwise. On a native build the variable is
      # not visible here (it is a normal variable owned by whichever scope
      # called `find_package(Corrosion)`), so the else-branch fired and this
      # glue crate was the ONE cargo invocation in a cmake build that spelled
      # the host build implicitly — while corrosion, in the same build tree,
      # always passes `--target`. Verified on a built native workspace:
      # `src/*/nano_ros_cpp_ffi_*/target/` held `nros-minsizerel/` with no
      # triple directory above it, next to corrosion trees that had one.
      #
      # Those two spellings are different cargo identities and share NOTHING,
      # not even through sccache (measured: 0 hits / 62 misses). The resolver
      # never returns empty, so the branch is gone and the artifact path always
      # carries the triple — which is also corrosion's layout, so the whole
      # cmake lane now reads one way. Rationale + numbers live at
      # `_nros_resolve_rust_target` in NanoRosCodegenCore.cmake.
      _nros_resolve_rust_target(_ffi_rust_target)
      set(_ffi_lib "${_ffi_target_dir}/${_ffi_rust_target}/${_NROS_FFI_DIR}/libnano_ros_cpp_ffi_${target}.a")
      # issue 0820 — cargo writes its dep-info beside the artifact, same
      # basename, `.d`. The two spellings move together like the profile and
      # the output path above: a `.d` derived from anything but `_ffi_lib`
      # would name a file cargo never wrote, and ninja tolerates a missing
      # depfile silently — which is the failure this edge exists to end.
      string(REGEX REPLACE "\\.a$" ".d" _ffi_dep_file "${_ffi_lib}")

      file(MAKE_DIRECTORY "${_ffi_crate_src}")

      # Generate Cargo.toml from template.
      # Phase 214.B.1 — emit a path RELATIVE to the FFI crate dir, not the
      # absolute repo path. Absolute paths broke clean clones + CI on
      # different paths. The Cargo.toml's `path = "..."` is interpreted
      # relative to the manifest dir, so a relative emit is portable.
      set(FFI_TARGET "${target}")
      file(RELATIVE_PATH SERDES_DIR "${_ffi_crate_dir}" "${_serdes_dir}")
      configure_file(
        "${_NANO_ROS_CMAKE_DIR}/cpp_ffi_Cargo.toml.in"
        "${_ffi_crate_dir}/Cargo.toml"
        @ONLY
      )

      # Generate lib.rs with include!() for cross-package FFI references — the
      # de-duplicated dep closure + own files, each include!()d so all
      # cross-package types share one flat module scope. De-dup + relative-path
      # emission live in the shared core (Phase 246; was copy-pasted, drifted in
      # issue 0052 + Phase 214.B.1).
      _nros_collect_rs_closure(_ffi_rs_all
        DEPS ${_ARG_DEPENDENCIES}
        OWN ${_generated_rs_files})
      _nros_write_ffi_lib_rs(
        CRATE_SRC "${_ffi_crate_src}"
        TEMPLATE "${_NANO_ROS_CMAKE_DIR}/ffi_lib_rs.in"
        RS_FILES ${_ffi_rs_all}
        PATH_MODE relative)

      # For Tier 3 targets (e.g. armv7a-nuttx-eabi), generate a .cargo/config.toml
      # with build-std=core and use nightly toolchain.
      set(_ffi_cargo_prefix "")
      # Reset per iteration: this runs inside a foreach over packages, and a
      # value left over from a previous target would silently drop `--target`.
      set(_ffi_target_in_config "")
      # Read the RESOLVED triple, not `Rust_CARGO_TARGET` directly: the resolver
      # falls back to Corrosion's cache copy, which survives the
      # `add_subdirectory()` boundary that a PARENT_SCOPE publish does not
      # (phase-155 — the normal variable went missing here and a NuttX build
      # quietly emitted host x86_64 objects into an ARM link).
      if(_ffi_rust_target MATCHES "nuttx")
        # Per-arch cross compiler for the generated config (#199 follow-up —
        # this block used to hardcode the arm pair, breaking the riscv board).
        # The env key is the triple with `-`→`_` (cc-rs convention).
        if(_ffi_rust_target MATCHES "^riscv32")
          set(_ffi_cross_gcc "riscv-none-elf-gcc")
        else()
          set(_ffi_cross_gcc "arm-none-eabi-gcc")
        endif()
        string(REPLACE "-" "_" _ffi_cc_env_key "${_ffi_rust_target}")
        file(MAKE_DIRECTORY "${_ffi_crate_dir}/.cargo")
        file(WRITE "${_ffi_crate_dir}/.cargo/config.toml"
          "[build]\ntarget = \"${_ffi_rust_target}\"\n\n"
          "[unstable]\nbuild-std = [\"core\"]\n\n"
          "[target.${_ffi_rust_target}]\nlinker = \"${_ffi_cross_gcc}\"\n\n"
          "[env]\nCC_${_ffi_cc_env_key} = \"${_ffi_cross_gcc}\"\n"
        )
        # Pin to the EXACT nightly the rest of the build uses — the dated
        # nightly is what's installed (matches examples/qemu-arm-nuttx/rust-
        # toolchain.toml + cmake/toolchain/armv7a-nuttx-eabi.cmake's
        # Rust_TOOLCHAIN). Generic `+nightly` resolves to an UNinstalled
        # `nightly-x86_64-unknown-linux-gnu` → rustlib src/Cargo.lock missing.
        set(_ffi_cargo_prefix "+${Rust_TOOLCHAIN}")
        # With .cargo/config.toml, `[build] target` is set there; don't pass
        # `--target` as well. The triple itself STAYS set — the artifact still
        # lands under `<target-dir>/<triple>/<profile>/` and `_ffi_lib` above
        # already spells it that way. (Blanking the variable here is what used
        # to make "no triple" ambiguous between this case and a host build.)
        set(_ffi_target_in_config TARGET_IN_CONFIG)
      endif()

      # Assemble cargo args via the shared core (Phase 246.3). phase-336: the
      # profile is the carve-out resolved above, not a hardcoded `release`.
      # build-std for nuttx comes from .cargo/config.toml above (not an inline
      # -Z), so no BUILD_STD here. The `+<toolchain>` prefix stays separate
      # (prepended in the COMMAND).
      _nros_ffi_cargo_args(_ffi_cargo_args
        MANIFEST "${_ffi_crate_dir}/Cargo.toml"
        TARGET_DIR "${_ffi_target_dir}"
        PROFILE "${_NROS_FFI_PROFILE}"
        RUST_TARGET "${_ffi_rust_target}"
        ${_ffi_target_in_config})

      # Build the FFI staticlib after codegen runs
      # The preset's definition has to ride ON the command: this is a plain
      # custom command, so `corrosion_set_env_vars` cannot reach it. Empty for a
      # user-owned profile, where the crate's own manifest governs.
      # phase-351 W5 — the message FFI glue is TARGET code, so its cargo
      # invocation carries the board facts like every other one. Cheap and
      # unconditional on purpose: deciding per crate whether it "needs" the
      # board rung is the judgment call that leaves one lane silently without
      # it, which is how the Zephyr arm shipped inert.
      nros_resolve_board_facts()
      set(_ffi_env "")
      if(NOT _NROS_FFI_ENV STREQUAL "")
        set(_ffi_env ${CMAKE_COMMAND} -E env ${_NROS_FFI_ENV} ${NROS_BOARD_FACTS_ENV})
      endif()
      # issue 0820 — DEPENDS names the crate's OWN files and nothing else, so
      # this archive had no edge on the nano-ros Rust it compiles in. Cargo's
      # dep-info lists `packages/core/nros-serdes/src/{cdr,lib,...}.rs` for
      # exactly this artifact (measured), and a hand-written DEPENDS is a
      # standing approximation of a graph cargo already computes exactly.
      # Without it, editing the CDR serializer left every generated message
      # staticlib holding the previous build's code, with a fresh mtime — the
      # museum-archive shape 0820 hit one seam over, on NuttX.
      add_custom_command(
        OUTPUT "${_ffi_lib}"
        COMMAND ${_ffi_env} cargo ${_ffi_cargo_prefix} ${_ffi_cargo_args}
        DEPENDS ${_generated_rs_files} "${_ffi_crate_dir}/Cargo.toml" "${_ffi_crate_src}/lib.rs"
        DEPFILE "${_ffi_dep_file}"
        WORKING_DIRECTORY "${_ffi_crate_dir}"
        COMMENT "Building Rust FFI glue for ${target} C++ bindings"
        VERBATIM
      )

      add_custom_target(${_lib_target}_ffi DEPENDS "${_ffi_lib}")
      add_dependencies(${_lib_target}_ffi ${_lib_target}_gen)
      # Ensure dependency codegen targets run before our FFI build
      foreach(_dep ${_ARG_DEPENDENCIES})
        if(TARGET ${_dep}__nano_ros_cpp_gen)
          add_dependencies(${_lib_target}_ffi ${_dep}__nano_ros_cpp_gen)
        endif()
      endforeach()
      add_dependencies(${_lib_target} ${_lib_target}_ffi)

      # Import the built staticlib. IMPORTED GLOBAL (issue #149): in a multi-package
      # WORKSPACE the interface lib is codegen'd in the NODE package's subdirectory
      # scope, but the entry package's `nros_board_link_app` (which walks LINK_INTERFACES
      # → `$<TARGET_FILE:<lib>_ffi_lib>` for the NuttX kernel link) runs in the ENTRY
      # subdirectory scope, where a non-GLOBAL IMPORTED target is invisible — so
      # `if(TARGET <lib>_ffi_lib)` was false and the C++ serdes `.a` never reached the
      # link line → `undefined reference to nros_cpp_publish_<msg>`. GLOBAL makes it
      # visible everywhere (standalone single-scope builds are unaffected).
      add_library(${_lib_target}_ffi_lib STATIC IMPORTED GLOBAL)
      set_target_properties(${_lib_target}_ffi_lib PROPERTIES
        IMPORTED_LOCATION "${_ffi_lib}"
      )
      target_link_libraries(${_lib_target} INTERFACE ${_lib_target}_ffi_lib)

      # Phase 150.B — record the ffi_lib → NanoRos::NanoRosCpp link
      # dependency explicitly so CMake's topological sort places
      # libnros_cpp.a AFTER the per-package ffi staticlib in the
      # final link line. (Phase 246.4: this INTERFACE-library ordering is
      # intentionally NOT shared with the Zephyr generator's whole-archive-onto-
      # `app` approach — opposite ld-order direction + different target model.) Without this, both libs land as sibling
      # INTERFACE deps of `${_lib_target}` with no recorded ordering;
      # CMake picks declaration order (libnros_cpp.a first), GNU ld
      # processes left→right and discards `nros_cpp_publish_raw` from
      # libnros_cpp.a before the ffi lib pulls it in, producing
      # `undefined reference to nros_cpp_publish_raw` at executable
      # link time. Declaring the dep both registers ordering AND
      # forwards NanoRosCpp's transitive deps (nros_c, nros_platform,
      # rmw staticlib) to consumers of the ffi lib.
      if(TARGET NanoRos::NanoRosCpp)
        set_property(TARGET ${_lib_target}_ffi_lib APPEND PROPERTY
          INTERFACE_LINK_LIBRARIES NanoRos::NanoRosCpp)
      elseif(TARGET nros_cpp::nros_cpp)
        set_property(TARGET ${_lib_target}_ffi_lib APPEND PROPERTY
          INTERFACE_LINK_LIBRARIES nros_cpp::nros_cpp)
      endif()
    endif()

    # Link to nros C++ library (prefer installed target, fall back to build-time Corrosion target)
    if(TARGET NanoRos::NanoRosCpp)
      target_link_libraries(${_lib_target} INTERFACE NanoRos::NanoRosCpp)
    elseif(TARGET nros_cpp::nros_cpp)
      target_link_libraries(${_lib_target} INTERFACE nros_cpp::nros_cpp)
    endif()

    # Link dependency libraries
    foreach(_dep ${_ARG_DEPENDENCIES})
      if(TARGET ${_dep}__nano_ros_cpp)
        target_link_libraries(${_lib_target} INTERFACE ${_dep}__nano_ros_cpp)
        target_include_directories(${_lib_target} INTERFACE
          "$<TARGET_PROPERTY:${_dep}__nano_ros_cpp,INTERFACE_INCLUDE_DIRECTORIES>")
      endif()
    endforeach()
  else()
    # C target with .c sources
    set(_lib_target "${target}__nano_ros_c")

    if(_generated_sources)
      add_library(${_lib_target} STATIC ${_generated_sources})
      target_include_directories(${_lib_target}
        PUBLIC
          $<BUILD_INTERFACE:${_output_dir}>
          $<BUILD_INTERFACE:${_umbrella_dir}>
          $<INSTALL_INTERFACE:include/${target}>
      )
      # Issue 0114 — the generated message `.c` TUs include <nros/nros_generated.h>
      # (→ the `*_OPAQUE_U64S` sizes), so on a parallel/fresh NATIVE build they can
      # compile BEFORE Corrosion's `nros_c_config_header` mirror custom command runs
      # and read the in-tree `#error` stub (`*_OPAQUE_U64S undeclared`). The mirror
      # dir is on the include path ahead of the stub, but the file is not there yet.
      # `add_dependencies` orders the target but not each TU (issues 0088/0090), so
      # also set a HARD file-level `OBJECT_DEPENDS` on the generated sources. Gate on
      # the Corrosion mirror actually existing rather than a platform name: posix AND
      # the threadx/riscv64 cross-Cyclone examples both build the sizes header via the
      # `nros_c_config_header` mirror target and hit this race (issue 0122). Zephyr and
      # the freertos carrier generate the header through other paths and define no such
      # target/property, so the guards below skip them (freertos is handled in
      # NanoRosNodeRegister.cmake, issue 0090).
      get_property(_nrgi_c_hdr GLOBAL PROPERTY NROS_C_CONFIG_HEADER_FILE)
      if(_nrgi_c_hdr AND TARGET nros_c_config_header)
        add_dependencies(${_lib_target} nros_c_config_header)
        # issue 0740 — a LOCAL stamp, not the cross-directory mirror path (see
        # `_nros_config_header_stamp`). Guarded: this module is reachable in
        # configurations where NanoRosNodeRegister.cmake was not included.
        if(COMMAND _nros_config_header_stamp)
          _nros_config_header_stamp(_nrgi_cfg_stamp "${_lib_target}" "${_nrgi_c_hdr}")
        else()
          set(_nrgi_cfg_stamp "${_nrgi_c_hdr}")
        endif()
        set_source_files_properties(${_generated_sources} PROPERTIES
          OBJECT_DEPENDS "${_nrgi_cfg_stamp}")
      endif()
    else()
      add_library(${_lib_target} INTERFACE)
      target_include_directories(${_lib_target}
        INTERFACE
          $<BUILD_INTERFACE:${_output_dir}>
          $<BUILD_INTERFACE:${_umbrella_dir}>
          $<INSTALL_INTERFACE:include/${target}>
      )
    endif()

    # Link to the nano-ros runtime umbrella.
    #
    # issue 0425 — prefer `NanoRosCpp` when it exists. This generated C binding
    # library is consumed by BOTH C and C++ packages, so in a mixed workspace it
    # was the third path (after the node-lib and entry sites) that dragged
    # `libnros_c.a` onto a C++ executable that already links `libnros_cpp.a` —
    # and the latter BUNDLES nros-c, so the two archives define the same C ABI
    # twice. Preferring the bundling umbrella keeps ONE Rust staticlib per
    # binary; a pure-C workspace has no `NanoRosCpp` target and is unchanged.
    if(TARGET NanoRos::NanoRosCpp)
      set(_link_type PUBLIC)
      if(NOT _generated_sources)
        set(_link_type INTERFACE)
      endif()
      target_link_libraries(${_lib_target} ${_link_type} NanoRos::NanoRosCpp)
    elseif(TARGET NanoRos::NanoRos)
      set(_link_type PUBLIC)
      if(NOT _generated_sources)
        set(_link_type INTERFACE)
      endif()
      target_link_libraries(${_lib_target} ${_link_type} NanoRos::NanoRos)
    elseif(TARGET nros_c::nros_c)
      set(_link_type PUBLIC)
      if(NOT _generated_sources)
        set(_link_type INTERFACE)
      endif()
      target_link_libraries(${_lib_target} ${_link_type} nros_c::nros_c)
    endif()

    # Link dependency libraries
    foreach(_dep ${_ARG_DEPENDENCIES})
      if(TARGET ${_dep}__nano_ros_c)
        set(_link_type PUBLIC)
        if(NOT _generated_sources)
          set(_link_type INTERFACE)
        endif()
        target_link_libraries(${_lib_target} ${_link_type} ${_dep}__nano_ros_c)
      endif()
    endforeach()

    # Build-order: the generated message .c files #include <nros/types.h> which
    # pulls <nros/nros_generated.h>, the cbindgen header nros-c's build.rs writes
    # during its cargo build. Linking NanoRos::NanoRos orders the LINK, not the
    # COMPILE, so without an explicit dependency these objects can compile before
    # the header exists — a race only masked when some other target happened to
    # build nros-c first (e.g. freertos' cyclone fixture step; nuttx, building
    # c/cpp first, hit `fatal error: nros/nros_generated.h: No such file`). Make
    # the message lib wait for the nros-c cargo build (corrosion's build target).
    if(_generated_sources AND TARGET cargo-build_nros_c)
      add_dependencies(${_lib_target} cargo-build_nros_c)
    endif()
  endif()

  # phase-347 W5 — per-message typesupport, DECLARED not named.
  #
  # This was ~180 lines guarded by `if(NANO_ROS_RMW STREQUAL "cyclonedds" ...)`:
  # one backend's idlc pipeline living in the pipeline every backend goes
  # through. It now lives in that backend's own package and is reached through
  # its descriptor (`[rmw.codegen].per_message`, RFC-0071 D4), surfaced by
  # `nros_rmw_dispatch` as `NROS_RMW_PER_MESSAGE_HOOK`.
  #
  # A backend that declares no hook pays nothing. The `COMMAND` guard stays: a
  # backend may declare a hook whose package has not been add_subdirectory'd in
  # this configure (cyclone's is defined only once its package is added), and
  # that is a legitimate no-op rather than an error.
  if(NANO_ROS_RMW AND NOT NANO_ROS_RMW STREQUAL "none")
    nros_rmw_dispatch("${NANO_ROS_RMW}")
    if(NROS_RMW_PER_MESSAGE_HOOK AND COMMAND ${NROS_RMW_PER_MESSAGE_HOOK})
      cmake_language(CALL ${NROS_RMW_PER_MESSAGE_HOOK}
        TARGET          "${target}"
        LIB_TARGET      "${_lib_target}"
        INTERFACE_FILES ${_interface_files}
        DEPENDENCIES    ${_ARG_DEPENDENCIES})
    endif()
  endif()

  # Install
  if(NOT _ARG_SKIP_INSTALL)
    if(_ARG_LANGUAGE STREQUAL "CPP")
      install(
        DIRECTORY "${_output_dir}/"
        DESTINATION "include/${target}"
        FILES_MATCHING PATTERN "*.hpp"
      )
    else()
      install(
        DIRECTORY "${_output_dir}/"
        DESTINATION "include/${target}"
        FILES_MATCHING PATTERN "*.h"
      )
      if(_generated_sources)
        install(TARGETS ${_lib_target}
          EXPORT ${target}Targets
          ARCHIVE DESTINATION lib
          LIBRARY DESTINATION lib
        )
      endif()
    endif()
    install(EXPORT ${target}Targets
      FILE ${target}Targets.cmake
      NAMESPACE ${target}::
      DESTINATION "lib/cmake/${target}"
    )
  endif()

  # Phase 220.G.2 — register the interface lib in a DIRECTORY-scoped
  # property so `nano_ros_node_register` can auto-link it without each
  # example having to do a manual `target_link_libraries(<component>
  # PUBLIC <pkg>__nano_ros_{c,cpp})` (the Phase 220.G.1 boilerplate).
  # DIRECTORY scope (not GLOBAL) so a workspace with multiple example
  # pkgs (or a colcon workspace) doesn't cross-pollinate one pkg's libs
  # into another pkg's component. Duplicates are de-duped at link time.
  # phase-263 C2c — on Zephyr the generated interface FFI is whole-archived into `app` by the
  # Zephyr generator (zephyr/cmake/nros_generate_interfaces.cmake), NOT exposed as a linkable
  # `<pkg>__nano_ros_cpp` lib; appending it here makes consumers try `-l<name>` → "cannot find
  # -lstd_msgs__nano_ros_cpp". Skip the registration on Zephyr (the headers reach component
  # libs via the `app` include mirror).
  if(NOT NANO_ROS_PLATFORM STREQUAL "zephyr")
    set_property(DIRECTORY APPEND PROPERTY
      NROS_GENERATED_INTERFACE_LIBS "${_lib_target}")
  endif()

  # Export variables for downstream
  set(${target}_INCLUDE_DIRS "${_output_dir}" PARENT_SCOPE)
  set(${target}_LIBRARIES "${_lib_target}" PARENT_SCOPE)
  set(${target}_GENERATED_HEADERS "${_generated_headers}" PARENT_SCOPE)
  set(${target}_GENERATED_SOURCES "${_generated_sources}" PARENT_SCOPE)
  # Carry the TRANSITIVE closure of generated FFI .rs files (own + every dep's,
  # de-duped) so a consumer that include!()s a direct dep also gets its nested
  # cross-package types. Computation + dedup live in the shared core (Phase 246).
  # The PARENT_SCOPE export must stay HERE (a helper function's PARENT_SCOPE
  # reaches only the helper's caller, not the user) — see NanoRosCodegenCore.cmake.
  _nros_collect_rs_closure(_rs_closure
    DEPS ${_ARG_DEPENDENCIES}
    OWN ${_generated_rs_files})
  set(${target}_GENERATED_RS_FILES "${_rs_closure}" PARENT_SCOPE)
  # INTERNAL CACHE stash for multi-level scope chains where PARENT_SCOPE (one
  # level) doesn't reach a sibling-call-tree consumer (Phase 210.E.3).
  _nros_export_rs_closure(${target} "${_rs_closure}")
  set(_NROS_PKG_${target}_GENERATED_HEADERS "${_generated_headers}"
      CACHE INTERNAL "nros cached GENERATED_HEADERS for ${target}" FORCE)
  set(_NROS_PKG_${target}_GENERATED_SOURCES "${_generated_sources}"
      CACHE INTERNAL "nros cached GENERATED_SOURCES for ${target}" FORCE)
  set(_NROS_PKG_${target}_INCLUDE_DIRS "${_output_dir}"
      CACHE INTERNAL "nros cached INCLUDE_DIRS for ${target}" FORCE)
endfunction()


# =========================================================================
# rosidl_generate_interfaces(<target> <files>...
#     [DEPENDENCIES <packages>...]
#     [LIBRARY_NAME <library>]
#     [SKIP_INSTALL]
#     [ADD_LINTER_TESTS]
#     [SKIP_GROUP_MEMBERSHIP_CHECK])
#
# Phase 210.A.1 — upstream-shape entry point. A standard ROS 2 msg pkg's
# CMakeLists.txt calls this verbatim — same name, same signature as the
# rosidl_default_generators function. nano-ros routes it to its own codegen
# (`nros_generate_interfaces`, CPP) without the rosidl runtime needed; the
# upstream-only arguments (LIBRARY_NAME / ADD_LINTER_TESTS /
# SKIP_GROUP_MEMBERSHIP_CHECK) are accepted + no-op'd.
#
# Emits the canonical `${target}::${target}` IMPORTED INTERFACE alias on
# top of `${target}__nano_ros_cpp` so a consumer's
# `target_link_libraries(<app> <pkg>::<pkg>)` line — also stock-ROS shape —
# resolves through the alias to the nano-ros codegen lib.
#
# A standard ROS msg package's CMakeLists.txt now contains zero nano-ros-
# specific lines: just `find_package(ament_cmake REQUIRED) +
# find_package(rosidl_default_generators REQUIRED) +
# rosidl_generate_interfaces(${PROJECT_NAME} <files>) + ament_package()`.
# Find-stubs satisfy the find_package() calls.
# =========================================================================
function(rosidl_generate_interfaces target)
  cmake_parse_arguments(_ROS
    "SKIP_INSTALL;ADD_LINTER_TESTS;SKIP_GROUP_MEMBERSHIP_CHECK"
    "LIBRARY_NAME"
    "DEPENDENCIES"
    ${ARGN}
  )

  # Pass-through args + flags onto our codegen function. Default LANGUAGE
  # is CPP — rosidl emits all languages, nano-ros routes through rclcpp_compat
  # so C++ is the front. Callers wanting C invoke nros_generate_interfaces
  # directly.
  #
  # SKIP_INSTALL defaults TRUE for the rosidl-wrapper path: nano-ros doesn't
  # publish the upstream `share/<pkg>/` install layout — consumers come in via
  # `find_package(<pkg>)` → smart Find-stub (Phase 210.A.2), not via an
  # install prefix on `AMENT_PREFIX_PATH`. Explicit `SKIP_INSTALL` keyword
  # is still honoured (it's already the default; harmless duplicate).
  set(_passthru_flags SKIP_INSTALL)

  if(_ROS_ADD_LINTER_TESTS)
    message(STATUS "rosidl_generate_interfaces(${target}): ADD_LINTER_TESTS no-op'd (ament linter stack not bundled).")
  endif()
  if(_ROS_SKIP_GROUP_MEMBERSHIP_CHECK)
    message(STATUS "rosidl_generate_interfaces(${target}): SKIP_GROUP_MEMBERSHIP_CHECK accepted (no-op).")
  endif()
  if(_ROS_LIBRARY_NAME)
    message(STATUS "rosidl_generate_interfaces(${target}): LIBRARY_NAME='${_ROS_LIBRARY_NAME}' accepted (nano-ros uses fixed `${target}__nano_ros_cpp` naming; alias `${target}::${target}` resolves it).")
  endif()

  nros_generate_interfaces(${target}
    ${_ROS_UNPARSED_ARGUMENTS}
    DEPENDENCIES ${_ROS_DEPENDENCIES}
    LANGUAGE CPP
    ${_passthru_flags}
  )

  # Upstream-shape consumer link target: `<pkg>::<pkg>`. Alias the codegen
  # interface lib so `target_link_libraries(<app> <pkg>::<pkg>)` works.
  if(TARGET ${target}__nano_ros_cpp AND NOT TARGET ${target}::${target})
    add_library(${target}::${target} ALIAS ${target}__nano_ros_cpp)
  endif()
  # Some upstream consumers write `<pkg>::<pkg>__rosidl_typesupport_cpp`.
  # Alias to the same target so the link still resolves.
  if(TARGET ${target}__nano_ros_cpp AND NOT TARGET ${target}::${target}__rosidl_typesupport_cpp)
    add_library(${target}::${target}__rosidl_typesupport_cpp ALIAS ${target}__nano_ros_cpp)
  endif()

  # Re-export the variables nros_generate_interfaces set in caller scope.
  set(${target}_INCLUDE_DIRS "${${target}_INCLUDE_DIRS}" PARENT_SCOPE)
  set(${target}_LIBRARIES "${${target}_LIBRARIES}" PARENT_SCOPE)
  set(${target}_GENERATED_HEADERS "${${target}_GENERATED_HEADERS}" PARENT_SCOPE)
  set(${target}_GENERATED_SOURCES "${${target}_GENERATED_SOURCES}" PARENT_SCOPE)
  set(${target}_GENERATED_RS_FILES "${${target}_GENERATED_RS_FILES}" PARENT_SCOPE)
endfunction()


# nros_find_interfaces() is defined in the shared core
# (NanoRosCodegenCore.cmake, included above) — Phase 246. It is
# platform-agnostic: it resolve-deps + topo-iterates, delegating to whichever
# `nros_generate_interfaces` the build loaded. (Was duplicated here + in
# zephyr/cmake/nros_find_interfaces.cmake.)


# =========================================================================
# nros_workspace_interfaces([PATHS <dir>…] [LANGUAGE C|CPP])
#
# Phase 210.B.2 — bulk orchestrator. Scans the layered interface-package
# search path (PATHS arg overrides NROS_INTERFACE_SEARCH_PATH if given),
# identifies every ROS msg package (member_of_group=rosidl_interface_packages
# OR has msg/srv/action dirs), topo-sorts by package.xml deps, then
# `add_subdirectory(<pkg-dir>)` each. Each pkg's own CMakeLists.txt runs
# (which calls `rosidl_generate_interfaces(...)`), wiring its codegen +
# emitting the `<pkg>::<pkg>` alias. Multi-pkg workspaces collapse to one
# call instead of N per-pkg `add_subdirectory(...)` lines.
#
# Idempotent: pkgs already wired (`TARGET ${pkg}__nano_ros_cpp`) are
# skipped.  Shadowing (workspace pkg vs AMENT) follows the search-path
# order — earlier roots win, with a `message(STATUS …)` line noting the
# shadow.
#
# Usage from an app's CMakeLists.txt:
#   set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_SOURCE_DIR}/src")
#   nros_workspace_interfaces()
#   find_package(my_app_msgs REQUIRED)
#   target_link_libraries(my_app PRIVATE my_app_msgs::my_app_msgs)
# =========================================================================
function(nros_workspace_interfaces)
  cmake_parse_arguments(_WS
    ""
    "LANGUAGE"
    "PATHS"
    ${ARGN}
  )

  if(NOT DEFINED _WS_LANGUAGE OR _WS_LANGUAGE STREQUAL "")
    set(_WS_LANGUAGE "CPP")
  endif()
  string(TOUPPER "${_WS_LANGUAGE}" _WS_LANGUAGE)

  # Resolve search roots — PATHS wins; else NROS_INTERFACE_SEARCH_PATH
  # cmake/env.
  set(_roots "")
  if(_WS_PATHS)
    list(APPEND _roots ${_WS_PATHS})
  endif()
  if(DEFINED NROS_INTERFACE_SEARCH_PATH AND NOT NROS_INTERFACE_SEARCH_PATH STREQUAL "")
    string(REPLACE ":" ";" _e "${NROS_INTERFACE_SEARCH_PATH}")
    list(APPEND _roots ${_e})
  endif()
  if(DEFINED ENV{NROS_INTERFACE_SEARCH_PATH} AND NOT "$ENV{NROS_INTERFACE_SEARCH_PATH}" STREQUAL "")
    string(REPLACE ":" ";" _e "$ENV{NROS_INTERFACE_SEARCH_PATH}")
    list(APPEND _roots ${_e})
  endif()

  if(NOT _roots)
    message(STATUS
      "nros_workspace_interfaces: no PATHS / NROS_INTERFACE_SEARCH_PATH set — nothing to scan.")
    return()
  endif()

  # 1) Scan: collect (pkg_name, pkg_dir, deps).
  set(_pkg_names "")
  set(_seen "")
  foreach(_root ${_roots})
    if(NOT IS_DIRECTORY "${_root}")
      continue()
    endif()
    # phase-360 W2 — a NEW msg package appearing under a scanned root is an
    # input to this discovery, same as a new `.msg` above.
    file(GLOB _pxs RELATIVE "${_root}" CONFIGURE_DEPENDS "${_root}/*/package.xml")
    foreach(_pxrel ${_pxs})
      get_filename_component(_pxdir "${_root}/${_pxrel}" DIRECTORY)
      nros_read_package_xml_body("${_root}/${_pxrel}" _pxbody)
      if(NOT _pxbody MATCHES "<name>[ \t\r\n]*([A-Za-z0-9_-]+)[ \t\r\n]*</name>")
        continue()
      endif()
      string(REGEX REPLACE ".*<name>[ \t\r\n]*([A-Za-z0-9_-]+)[ \t\r\n]*</name>.*" "\\1" _pname "${_pxbody}")
      # Skip non-msg pkgs.
      set(_is_msg FALSE)
      if(_pxbody MATCHES "<member_of_group>[ \t\r\n]*rosidl_interface_packages[ \t\r\n]*</member_of_group>")
        set(_is_msg TRUE)
      elseif(IS_DIRECTORY "${_pxdir}/msg" OR IS_DIRECTORY "${_pxdir}/srv" OR IS_DIRECTORY "${_pxdir}/action")
        set(_is_msg TRUE)
      endif()
      if(NOT _is_msg)
        continue()
      endif()
      # Shadowing — first root with this pkg name wins.
      if("${_pname}" IN_LIST _seen)
        message(STATUS
          "nros_workspace_interfaces: ${_pname} found in multiple roots — keeping earlier; shadowed copy at ${_pxdir}")
        continue()
      endif()
      list(APPEND _seen "${_pname}")
      list(APPEND _pkg_names "${_pname}")
      set(_pkg_${_pname}_dir "${_pxdir}")
      # Parse deps so we can topo-sort.
      _nros_parse_pkg_deps_inline("${_root}/${_pxrel}" _d)
      # Filter to deps that are ALSO in this workspace — cross-workspace
      # deps (e.g. std_msgs from AMENT) go through find_package's smart
      # stub when the per-pkg add_subdirectory runs, not here.
      set(_wd "")
      foreach(_dep ${_d})
        if("${_dep}" IN_LIST _seen
            OR EXISTS "${_root}/${_dep}/package.xml")
          list(APPEND _wd "${_dep}")
        endif()
      endforeach()
      set(_pkg_${_pname}_deps "${_wd}")
    endforeach()
  endforeach()

  if(NOT _pkg_names)
    message(STATUS "nros_workspace_interfaces: no msg packages found under ${_roots}.")
    return()
  endif()

  # 2) Topo-sort. Kahn's algorithm — repeatedly pick a pkg whose deps are
  # all already emitted. Detects cycles (would leave pkgs un-emitted).
  set(_ordered "")
  set(_remaining "${_pkg_names}")
  set(_iter 0)
  list(LENGTH _remaining _rcount)
  while(_rcount GREATER 0)
    math(EXPR _iter "${_iter} + 1")
    set(_picked_this_round FALSE)
    foreach(_p ${_remaining})
      set(_unsat FALSE)
      foreach(_d ${_pkg_${_p}_deps})
        if(NOT "${_d}" IN_LIST _ordered)
          set(_unsat TRUE)
          break()
        endif()
      endforeach()
      if(NOT _unsat)
        list(APPEND _ordered "${_p}")
        list(REMOVE_ITEM _remaining "${_p}")
        set(_picked_this_round TRUE)
      endif()
    endforeach()
    if(NOT _picked_this_round)
      message(WARNING
        "nros_workspace_interfaces: dep cycle (or missing dep) among ${_remaining}; emitting in scan order.")
      list(APPEND _ordered ${_remaining})
      set(_remaining "")
    endif()
    list(LENGTH _remaining _rcount)
    if(_iter GREATER 100)
      message(FATAL_ERROR "nros_workspace_interfaces: topo-sort iteration cap reached — bug.")
    endif()
  endwhile()

  # 3) add_subdirectory each pkg in topo order. The pkg's CMakeLists.txt
  # calls rosidl_generate_interfaces(...) which wires the codegen.
  foreach(_p ${_ordered})
    if(TARGET ${_p}__nano_ros_cpp)
      # Already wired (e.g. by an earlier find_package call); skip.
      continue()
    endif()
    set(_pkg_dir "${_pkg_${_p}_dir}")
    set(_bin_dir "${CMAKE_CURRENT_BINARY_DIR}/nros-ws-${_p}")
    message(STATUS "nros_workspace_interfaces: building ${_p} from ${_pkg_dir}")
    add_subdirectory("${_pkg_dir}" "${_bin_dir}")
  endforeach()
endfunction()


# Inline helper — parses package.xml deps without recursion (mirror of the
# helper inside _NrosFindRosMsgPackage.cmake; keep here so the workspace
# function works even if the smart stub hasn't been loaded yet).
function(_nros_parse_pkg_deps_inline pxml out_var)
  set(_deps "")
  if(EXISTS "${pxml}")
    nros_read_package_xml_body("${pxml}" _body)
    string(REGEX MATCHALL "<(depend|build_depend|exec_depend|run_depend|build_export_depend)[^>]*>[ \t\r\n]*([A-Za-z0-9_-]+)[ \t\r\n]*</(depend|build_depend|exec_depend|run_depend|build_export_depend)>" _matches "${_body}")
    foreach(_m ${_matches})
      string(REGEX REPLACE "<[^>]+>[ \t\r\n]*([A-Za-z0-9_-]+)[ \t\r\n]*</[^>]+>" "\\1" _name "${_m}")
      if(NOT _name MATCHES "^(rosidl|ament|rclcpp|rclpy|rcl|rmw|rosgraph|launch|catkin)")
        list(APPEND _deps "${_name}")
      endif()
    endforeach()
    list(REMOVE_DUPLICATES _deps)
  endif()
  set(${out_var} "${_deps}" PARENT_SCOPE)
endfunction()
