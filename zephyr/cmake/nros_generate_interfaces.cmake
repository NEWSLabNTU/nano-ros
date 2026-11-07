#[=======================================================================[.rst:
# phase-336 — the shared cargo-profile resolver (`nros dev profile`), included
# at FILE scope so a function body never include()s inside its own frame.
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosCargoProfile.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosRustToolchain.cmake")  # issue 2312
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosBoardFacts.cmake")
# Issue 1547 — the one edition resolver, at FILE scope for the same reason.
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosRosEdition.cmake")

nros_generate_interfaces (Zephyr)
---------------------------------

Generate C or C++ bindings for ROS 2 interface files (.msg, .srv, .action)
in Zephyr applications.

This module provides ``nros_generate_interfaces()`` for Zephyr builds.
It reuses the same ``nros-codegen`` binary as the native CMake workflow
but adds generated sources directly to the Zephyr ``app`` target instead
of creating a standalone static library.

.. code-block:: cmake

  nros_generate_interfaces(<target>
    [<interface_files>...]
    [LANGUAGE C|CPP]
    [DEPENDENCIES <packages>...]
  )

Arguments:
  ``<target>``
    Package name for the generated bindings (e.g., ``std_msgs``).
  ``<interface_files>``
    Relative paths to .msg, .srv, or .action files
    (e.g., ``msg/Int32.msg``).  Each file is resolved in order:

    1. ``${CMAKE_CURRENT_SOURCE_DIR}/<file>``  (local)
    2. ``${prefix}/share/<target>/<file>``      (ament index)

    If no files are specified, auto-discovers from local ``msg/``,
    ``srv/``, ``action/`` directories and the ament index.
  ``LANGUAGE``
    Target language: ``C`` (default) or ``CPP``.
  ``DEPENDENCIES``
    List of interface packages this package depends on.

Prerequisites:
  ``nros-codegen`` is located in order:

  1. ``_NANO_ROS_CODEGEN_TOOL`` cache var (set on the cmake command
     line via ``west build -- -D_NANO_ROS_CODEGEN_TOOL=...``)
  2. ``CONFIG_NROS_CODEGEN_TOOL`` — set in ``prj.conf``
  3. ``nros-codegen`` on ``PATH``

  Build a host-side binary first via a parallel POSIX configure
  (see Kconfig help for ``NROS_CODEGEN_TOOL``).

#]=======================================================================]

# Phase 246 — shared codegen helpers (lib.rs assembly, rs-closure collect/export)
# from the repo's `cmake/` dir (this file lives at `zephyr/cmake/`). include_guard'd
# in the core, so a build that also pulls the canonical generator is fine.
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosCodegenCore.cmake")
# issue 2092 — the image's message-FFI staticlib is a SECOND Rust staticlib.
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosSecondStaticlib.cmake")

# =========================================================================
# Locate nros-codegen (once per configure)
# =========================================================================

# 1. Pre-set cache var: west build -- -D_NANO_ROS_CODEGEN_TOOL=...
#
# This value is supplied by the just recipes and may change when the global
# Cargo profile changes. Prefer it over the internal cache so existing Zephyr
# build directories do not keep pointing at an old/nonexistent codegen binary
# such as target/release/nros-codegen after the default profile moved to
# the profile the build resolved (phase-336).
if(DEFINED _NANO_ROS_CODEGEN_TOOL AND NOT _NANO_ROS_CODEGEN_TOOL STREQUAL "")
  if(NOT EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
    message(FATAL_ERROR
      "_NANO_ROS_CODEGEN_TOOL points at a missing nros-codegen binary:\n"
      "  ${_NANO_ROS_CODEGEN_TOOL}\n"
      "Rebuild the host codegen tool or update the CMake cache path.")
  endif()
  if(NOT _NROS_ZEPHYR_CODEGEN_TOOL STREQUAL _NANO_ROS_CODEGEN_TOOL)
    set(_NROS_ZEPHYR_CODEGEN_TOOL "${_NANO_ROS_CODEGEN_TOOL}")
    set(_NROS_ZEPHYR_CODEGEN_TOOL "${_NROS_ZEPHYR_CODEGEN_TOOL}"
      CACHE INTERNAL "Path to nros CLI (Zephyr)" FORCE)
  endif()
endif()

# 2. Kconfig pre-set (Zephyr-specific): CONFIG_NROS_CODEGEN_TOOL in prj.conf.
if(NOT _NROS_ZEPHYR_CODEGEN_TOOL
   AND DEFINED CONFIG_NROS_CODEGEN_TOOL
   AND NOT CONFIG_NROS_CODEGEN_TOOL STREQUAL "")
  set(_NROS_ZEPHYR_CODEGEN_TOOL "${CONFIG_NROS_CODEGEN_TOOL}")
endif()

# 3. Shared find/validate/cache (Phase 246.2b) — drops a stale cached path then
# find_program on PATH + ~/.nros/bin, FATAL if absent. No-op when the west `-D`
# pre-set (above) or Kconfig already populated the var. Cache name is the
# Zephyr-specific `_NROS_ZEPHYR_CODEGEN_TOOL` (read by nros_find_interfaces.cmake).
_nros_resolve_codegen_tool(_NROS_ZEPHYR_CODEGEN_TOOL)

# _nros_zephyr_resolve_interface(<target> <relpath> <out_var>) — thin wrapper
# over the shared core resolver (Phase 246.2b). Supplies the bundled-interface
# prefix (the nano-ros repo root, = this file's dir up two), which gives the
# Zephyr tree the bundled fallback tier it previously lacked — a no-op in-tree
# (no `share/nano-ros/interfaces/`) but functional for an installed layout.
function(_nros_zephyr_resolve_interface target relpath out_var)
  get_filename_component(_nros_repo "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../.." ABSOLUTE)
  _nros_resolve_interface_file("${target}" "${relpath}" _r
    BUNDLED_PREFIX "${_nros_repo}")
  set(${out_var} "${_r}" PARENT_SCOPE)
endfunction()

# =========================================================================
# nros_generate_interfaces(<target> [<files>...]
#     [LANGUAGE C|CPP] [DEPENDENCIES <deps>...])
# =========================================================================
function(nros_generate_interfaces target)
  cmake_parse_arguments(_ARG
    "SKIP_INSTALL;NO_FFI_CRATE"
    "ROS_EDITION;LANGUAGE;CODEGEN_CONFIG"
    "DEPENDENCIES"
    ${ARGN}
  )
  # NO_FFI_CRATE (issue 0253) — DEPRECATED no-op everywhere since phase-306 W1:
  # the split types/exports codegen makes every per-pkg crate export only its
  # own `nros_cpp_*` symbols, so nothing needs suppressing. Accepted so
  # existing callers don't break.
  # Phase 210.E.3.c — SKIP_INSTALL accepted for parity with the canonical
  # `cmake/NanoRosGenerateInterfaces.cmake` (which the smart Find-stub
  # passes unconditionally via rosidl_generate_interfaces wrapper).
  # Zephyr emits directly to the `app` target — there's no install layout
  # — so the flag is recognised + silently ignored.

  # Issue 1547 — explicit ROS_EDITION > NROS_ROS_EDITION > the default,
  # via the one resolver. The literal fallback here skipped the middle rung,
  # the phase-405 W3 defect the canonical generator had already shed, and it
  # also accepted an unknown edition silently where the resolver refuses it.
  _nros_resolve_ros_edition("${_ARG_ROS_EDITION}" _ARG_ROS_EDITION)

  if(NOT DEFINED _ARG_LANGUAGE OR _ARG_LANGUAGE STREQUAL "")
    set(_ARG_LANGUAGE "C")
  endif()
  # Normalize case exactly like the canonical generator does — the ament verb
  # passes the inferred language in lowercase (`cpp`), and every branch below
  # compares against "CPP" (287-W6: a lowercase value silently fell into the C
  # branch and the leaf failed on a missing std_msgs.hpp).
  string(TOUPPER "${_ARG_LANGUAGE}" _ARG_LANGUAGE)

  # phase-263 C2c — idempotent across node pkgs that share an interface. A multi-node Zephyr
  # workspace entry add_subdirectory's several node pkgs that each `nros_find_interfaces` the
  # same msg pkg (e.g. talker + listener both → std_msgs); without this guard the 2nd call
  # re-creates `<target>_<lang>_ffi_build` → "add_custom_target cannot create target …
  # already exists". The first call generates the headers + FFI staticlib and links them to
  # `app`; a later call for the same (target, language) is a no-op — consumers (the component
  # libs) reach the generated headers via app's include set (the C2d entry/app mirror).
  string(TOLOWER "${_ARG_LANGUAGE}" _nros_iface_lang)
  #
  # issue 1675 — keyed on a GLOBAL PROPERTY, not on `<target>_<lang>_ffi_build`.
  # Only the C++ path ever creates an `_ffi_build` target (it builds an FFI
  # staticlib); the C path builds none, so for C this guard could never fire and
  # every node package regenerated the shared interface packages' C bindings into
  # the one `app`. A multi-node C workspace entry therefore compiled `std_msgs`
  # and `builtin_interfaces` once per node package. The global
  # `--allow-multiple-definition` hid that until issue 1636 scoped the flag to C++
  # FFI images; after it, every Zephyr C workspace entry failed to link.
  # The property is language-keyed, so it says the same thing for both paths;
  # the TARGET check stays as a second witness for C++.
  get_property(_nros_iface_done GLOBAL PROPERTY
      "_NROS_IFACE_GENERATED_${target}_${_nros_iface_lang}")
  if(_nros_iface_done OR TARGET ${target}_${_nros_iface_lang}_ffi_build)
    return()
  endif()
  set_property(GLOBAL PROPERTY
      "_NROS_IFACE_GENERATED_${target}_${_nros_iface_lang}" TRUE)

  # --- Resolve or auto-discover interface files ---
  set(_interface_files "")

  if(_ARG_UNPARSED_ARGUMENTS)
    # Explicit files: resolve each via local + ament
    foreach(_relpath ${_ARG_UNPARSED_ARGUMENTS})
      _nros_zephyr_resolve_interface("${target}" "${_relpath}" _abs_path)
      if(_abs_path STREQUAL "NOTFOUND")
        message(FATAL_ERROR
          "nros_generate_interfaces(): cannot find '${_relpath}' for "
          "package '${target}'.\n"
          "  Searched:\n"
          "    ${CMAKE_CURRENT_SOURCE_DIR}/${_relpath}\n"
          "    AMENT_PREFIX_PATH/share/${target}/${_relpath}\n"
          "  Hint: source your ROS 2 setup.bash or set AMENT_PREFIX_PATH.")
      endif()
      list(APPEND _interface_files "${_abs_path}")
    endforeach()
  else()
    # phase-363 (W2's class, re-swept 2026-08-19) — CONFIGURE_DEPENDS on every
    # glob, for the same reason as `cmake/NanoRosGenerateInterfaces.cmake`: a
    # plain `file(GLOB)` runs at CONFIGURE time only, so ADDING a `.msg` leaves
    # the build generating the OLD set until something unrelated forces a
    # reconfigure. The generated sources are a function of this list, so a stale
    # list is a museum artifact.
    #
    # This is the Zephyr COPY of that function, and W2 never opened it — which is
    # the pattern phase-363 records: every re-sweep found the class surviving in
    # a sibling file. Unlike the vendored ThreadX/Cyclone source globs, these
    # name USER content that changes with no submodule bump to reconfigure
    # behind it.
    # Auto-discover from local directories
    file(GLOB _local_msg CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/msg/*.msg")
    file(GLOB _local_srv CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/srv/*.srv")
    file(GLOB _local_action CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/action/*.action")
    list(APPEND _interface_files ${_local_msg} ${_local_srv} ${_local_action})

    # Fall back to ament index
    if(NOT _interface_files AND DEFINED ENV{AMENT_PREFIX_PATH})
      string(REPLACE ":" ";" _ament_paths "$ENV{AMENT_PREFIX_PATH}")
      foreach(_prefix ${_ament_paths})
        file(GLOB _ament_msg CONFIGURE_DEPENDS "${_prefix}/share/${target}/msg/*.msg")
        file(GLOB _ament_srv CONFIGURE_DEPENDS "${_prefix}/share/${target}/srv/*.srv")
        file(GLOB _ament_action CONFIGURE_DEPENDS "${_prefix}/share/${target}/action/*.action")
        list(APPEND _interface_files ${_ament_msg} ${_ament_srv} ${_ament_action})
      endforeach()
    endif()

    if(NOT _interface_files)
      message(FATAL_ERROR
        "nros_generate_interfaces(): no interface files found for '${target}'.\n"
        "  Searched: ${CMAKE_CURRENT_SOURCE_DIR}/{msg,srv,action}/ and AMENT_PREFIX_PATH.\n"
        "  Hint: add msg/Int32.msg locally or source ROS 2 setup.bash.")
    endif()
  endif()

  # Issue 2278 — a bundled copy only behind the opt-in (see the core's
  # `_nros_refuse_bundled_interfaces`). This tree's prefix is the repo root.
  get_filename_component(_nros_bundled_prefix "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../.." ABSOLUTE)
  _nros_refuse_bundled_interfaces(${target} "${_nros_bundled_prefix}" ${_interface_files})

  # --- Output directory ---
  if(_ARG_LANGUAGE STREQUAL "CPP")
    set(_output_dir "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp/${target}")
  else()
    set(_output_dir "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c/${target}")
  endif()

  file(MAKE_DIRECTORY "${_output_dir}")
  file(MAKE_DIRECTORY "${_output_dir}/msg")
  file(MAKE_DIRECTORY "${_output_dir}/srv")
  file(MAKE_DIRECTORY "${_output_dir}/action")

  # ---- Build JSON arguments file ----
  string(TOLOWER "${_ARG_LANGUAGE}" _lang_lower)
  set(_args_file "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_generate_${_lang_lower}_args__${target}.json")

  # Build + write the codegen args JSON (shared core, Phase 246.2). CODEGEN_CONFIG
  # (RFC-0033 per-field capacity) is now accepted here too (246.2b parity); empty
  # → no JSON field, identical to before for callers that don't pass it.
  _nros_write_codegen_args_json(
    ARGS_FILE "${_args_file}"
    PACKAGE "${target}"
    OUTPUT_DIR "${_output_dir}"
    ROS_EDITION "${_ARG_ROS_EDITION}"
    CODEGEN_CONFIG "${_ARG_CODEGEN_CONFIG}"
    INTERFACE_FILES ${_interface_files}
    DEPS ${_ARG_DEPENDENCIES})

  # Predict codegen outputs for the mtime "needs-regen" check below (shared
  # core, Phase 246.2). Order is irrelevant here — only EXISTS/mtime is tested.
  _nros_predict_generated_outputs(_exp_hdr _exp_src _exp_rs
    LANGUAGE "${_ARG_LANGUAGE}"
    PACKAGE "${target}"
    OUTPUT_DIR "${_output_dir}"
    INTERFACE_FILES ${_interface_files})
  # phase-403 W6 — the derived per-type size bounds. In the expected set so a
  # tree generated before this wave (headers present, no inventory) re-runs
  # codegen instead of silently having no bounds to read.
  #
  # This lane runs codegen at CONFIGURE time, so the fragment EXISTS by the time
  # anything downstream configures — which is what makes it usable for a Kconfig
  # value. See nros_message_bounds_files() in NanoRosCodegenCore.cmake.
  nros_message_bounds_files("${_output_dir}" _bounds_json _bounds_cmake)
  set(_expected_outputs ${_exp_hdr} ${_exp_src} ${_exp_rs}
      "${_bounds_json}" "${_bounds_cmake}")
  set(${target}_MESSAGE_BOUNDS_JSON "${_bounds_json}" CACHE INTERNAL
      "phase-403 W6: derived per-type serialized-size bounds for ${target}")
  set(${target}_MESSAGE_BOUNDS_CMAKE "${_bounds_cmake}" CACHE INTERNAL
      "phase-403 W6: include() this to read ${target}'s bounds as CMake variables")
  # phase-403 W8 (issue 0940) -- and now something READS them. Registered AFTER
  # codegen has run below, not here: a fragment that does not exist yet is a
  # FATAL_ERROR in the composer, and this lane's whole advantage is that the
  # file is on disk before anything downstream configures.

  # ---- Run codegen at configure time ----
  # Phase 196.1 — the codegen CLI is the `nros dev codegen` subcommand (Phase 195
  # folded the standalone nros-codegen binary into it); invoke it as such.
  nros_codegen_env_prefix(_codegen_env)
  if(_ARG_LANGUAGE STREQUAL "CPP")
    set(_codegen_cmd ${_codegen_env} "${_NROS_ZEPHYR_CODEGEN_TOOL}" dev codegen --language cpp --args-file "${_args_file}")
    message(STATUS "Generating nros C++ interfaces for ${target}")
  else()
    set(_codegen_cmd ${_codegen_env} "${_NROS_ZEPHYR_CODEGEN_TOOL}" dev codegen --args-file "${_args_file}")
    message(STATUS "Generating nros C interfaces for ${target}")
  endif()

  # issue 1018 — the IS_NEWER_THAN loop below is the right predicate and it was
  # unreachable: it only runs when a configure runs, and nothing made a
  # configure run when the tool moved. Register the emitter's own tool as a
  # configure dependency so this lane's freshness is a property of THIS call
  # site, not of whether the image also called `nano_ros_entry()`.
  nros_codegen_tool_reconfigure("${_NROS_ZEPHYR_CODEGEN_TOOL}")

  # issue 1470 — every `nros-codegen.toml` that reaches this generation (the
  # discovered chain plus an explicit CODEGEN_CONFIG that exists) is an input to
  # the BOUNDS in every generated header, and the args file changes only when a
  # config appears or moves. Same set the canonical lane hangs on
  # `add_custom_command`'s DEPENDS; `_nros_write_codegen_args_json` composes it.
  # The predicate is shared with the canonical lane's fragment pre-emit (issue
  # 1647), so the two lanes cannot disagree on what "stale" means.
  nros_codegen_run_stamp(_run_stamp "${_args_file}")
  nros_codegen_outputs_stale(_codegen_needed
    STAMP "${_run_stamp}"
    OUTPUTS ${_expected_outputs}
    INPUTS ${_interface_files} "${_args_file}" "${_NROS_ZEPHYR_CODEGEN_TOOL}"
           ${_NROS_CODEGEN_CONFIG_CHAIN})

  # RFC-0104 D5 / issue 1780 — and the ament inputs by CONTENT. The interface
  # files above are inputs by mtime only, which an apt upgrade (the `.deb`'s
  # build time) and a newly shadowing overlay both defeat. Same helper, same
  # CLI hash as the canonical lane and the cargo road.
  if(_ARG_LANGUAGE STREQUAL "CPP")
    set(_ament_lang cpp)
  else()
    set(_ament_lang c)
  endif()
  nros_codegen_ament_stale(_ament_stale
    TOOL "${_NROS_ZEPHYR_CODEGEN_TOOL}" LANGUAGE "${_ament_lang}" ARGS_FILE "${_args_file}"
    OUTPUT_DIR "${_output_dir}")
  if(_ament_stale)
    set(_codegen_needed TRUE)
  endif()

  # issue 1360 — and the EMITTED CODEGEN VERSION, which no mtime can express.
  #
  # This lane's build dirs live in the provisioned Zephyr workspace
  # (`$NROS_HOME/workspaces/zephyr/<version>`, RFC-0095 D4), which outlives
  # every checkout: both edges above are keyed on the tool THIS DIRECTORY cached,
  # and on the self-hosted runner that is the provisioning checkout's binary,
  # which CI never rebuilds. So a `NROS_CODEGEN_VERSION_MIN` move left every
  # fixture compiling against museum trees and failing on the RFC-0090 `#error`,
  # nightly after nightly, with all three freshness inputs reading current.
  # Asking the artifacts what version they state is the one input that is a
  # property of the tree on disk rather than of a timestamp.
  nros_codegen_version_stale(_version_stale
    CONTEXT "nros dev codegen (${target}, ${_ARG_LANGUAGE})"
    FILES ${_exp_hdr})
  if(_version_stale)
    set(_codegen_needed TRUE)
  endif()

  if(_codegen_needed)
    execute_process(
      COMMAND ${_codegen_cmd}
      WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
      RESULT_VARIABLE _codegen_result
      OUTPUT_VARIABLE _codegen_output
      ERROR_VARIABLE  _codegen_error
    )
  else()
    set(_codegen_result 0)
    set(_codegen_output "")
    set(_codegen_error "")
  endif()

  if(NOT _codegen_result EQUAL 0)
    message(FATAL_ERROR
      "nros-codegen failed for ${target} (exit ${_codegen_result}):\n"
      "  command: ${_codegen_cmd}\n"
      "  stdout: ${_codegen_output}\n"
      "  stderr: ${_codegen_error}")
  endif()
  if(_codegen_needed)
    # issue 1803 — the predicate above compares against this, not the outputs.
    nros_codegen_touch_run_stamp("${_args_file}")
  endif()

  # issue 1360 — close the loop. A tree still stating a refused version after a
  # successful regeneration means the cached tool is not this runtime's emitter,
  # and re-running it forever is the loop this arm exists to stop. Fail with the
  # binary and both numbers named, at configure time.
  if(_version_stale)
    nros_codegen_version_assert_fresh("${_NROS_ZEPHYR_CODEGEN_TOOL}"
      CONTEXT "nros dev codegen (${target}, ${_ARG_LANGUAGE})"
      FILES ${_exp_hdr})
  endif()

  # phase-403 W8 (issue 0940) -- codegen has run, so the bound fragment is on
  # disk. Register it for the image-wide compose in `nros_find_interfaces()`.
  nros_message_bounds_register_fragment("${_bounds_cmake}")

  # ---- Language-specific post-processing ----
  if(_ARG_LANGUAGE STREQUAL "CPP")
    # -- C++ path: header-only .hpp + Rust FFI staticlib --

    # Issue 1857 — the files codegen EMITS for this target, by prediction,
    # never a glob of the output dir. Codegen never deletes an output it no
    # longer writes, and this dir lives in a build tree that outlives renames
    # (issue 1360's persistent workspace), so a glob compiles every orphan
    # beside its replacement: phase-306's `*_ffi.rs` split (which once needed a
    # deletion here, E0428) and phase-492's C renames both. The canonical
    # generator (`NanoRosGenerateInterfaces.cmake`) already builds from these
    # same lists, so they are proven complete for both languages.
    set(_generated_headers ${_exp_hdr})
    set(_generated_rs_files ${_exp_rs})

    # Propagate the per-target generated-Rust file list to the caller
    # scope so a sibling nros_generate_interfaces() call that lists
    # this target under DEPENDENCIES can find it (the dep walk at
    # line ~318 below reads `${_dep}_GENERATED_RS_FILES`). Without
    # this set the cross-package FFI include!() chain was empty and
    # every consumer that referenced a type from a sibling package
    # failed to compile.
    # Build the TRANSITIVE closure (own files + every dep's closure), de-duped,
    # via the shared core (Phase 246 — identical computation to the canonical
    # generator). The PARENT_SCOPE export stays here (helper PARENT_SCOPE only
    # reaches this generator, not the user — see NanoRosCodegenCore.cmake).
    _nros_collect_rs_closure(_rs_closure
      DEPS ${_ARG_DEPENDENCIES}
      OWN ${_generated_rs_files})
    set(${target}_GENERATED_RS_FILES "${_rs_closure}" PARENT_SCOPE)
    _nros_export_rs_closure(${target} "${_rs_closure}")

    if(NOT _generated_headers)
      message(FATAL_ERROR
        "nros-codegen produced no .hpp files for ${target} in ${_output_dir}")
    endif()

    # Add include directories for generated headers
    target_include_directories(app PRIVATE
      "${_output_dir}"
      "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp"
    )
    # phase-518 — and for every other target of the image. A ported package's
    # own libraries (ros2/demos composition's `talker_component`) are compiled
    # outside `app` and include `<std_msgs/msg/string.hpp>` too; they reach the
    # headers by linking `<pkg>::<pkg>`, which the find module aliases onto
    # this target. The sources stay in `app`: this carries include paths only.
    if(NOT TARGET ${target}__nano_ros_cpp)
      add_library(${target}__nano_ros_cpp INTERFACE)
      target_include_directories(${target}__nano_ros_cpp INTERFACE
        "${_output_dir}"
        "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp"
      )
      # A package's headers include its dependencies' (`std_msgs/msg/header.hpp`
      # includes `builtin_interfaces.hpp`), so the dependencies' include paths
      # travel with it. A dependency is generated before its dependents, so its
      # target exists here (phase-518 W4, node 1 on Zephyr).
      foreach(_dep IN LISTS _ARG_DEPENDENCIES)
        if(TARGET ${_dep}__nano_ros_cpp)
          target_link_libraries(${target}__nano_ros_cpp INTERFACE ${_dep}__nano_ros_cpp)
        endif()
      endforeach()
      # And the C++ runtime's headers, which every generated header includes
      # (`rclcpp/fixed_string.hpp`): a library that links only the messages
      # (`target_link_libraries(<lib> PUBLIC ${std_msgs_TARGETS})`) and not
      # rclcpp had none of them. The umbrella helper picks the target, as
      # `rclcpp::rclcpp` does (issue 1467).
      if(TARGET NanoRos::NanoRosCpp)
        include("${NROS_REPO_DIR}/cmake/NanoRosRuntimeUmbrella.cmake")
        nros_link_runtime_umbrella(${target}__nano_ros_cpp INTERFACE
          CANDIDATES NanoRos::NanoRosCpp)
      endif()
    endif()

    # Build Rust FFI glue for generated message types
    if(_generated_rs_files)
      # Issue 1645 — ONE message-FFI staticlib per IMAGE, not one per package.
      #
      # Each package used to build its own `libnano_ros_cpp_ffi_<pkg>.a` and
      # whole-archive it into `app`. A Rust staticlib bundles its whole closure
      # (`compiler_builtins`, and `core` / `anon.*` constants on the host
      # triple), and whole-archiving forces every member in, so two packages
      # meant two copies of every intrinsic: `examples/zephyr/cpp/talker`
      # (std_msgs + builtin_interfaces) measured 392 duplicate symbols on
      # native_sim/native/64 and 571 on mps2/an385 with the flag removed, all
      # FFI-vs-FFI — which is why this file carried the image's
      # `--allow-multiple-definition`.
      #
      # So a package now only CONTRIBUTES its closure (dependency TYPES + its
      # own EXPORTS, phase-306 W1's split), and one deferred call at the end of
      # the app directory builds a single crate over the de-duplicated union
      # and whole-archives it once. The split is what makes the union safe:
      # an exports file belongs to exactly one package, so no `nros_cpp_*`
      # symbol can appear twice, and a types file shared by two closures is
      # included once.
      _nros_collect_rs_closure(_ffi_rs_pkg
        DEPS ${_ARG_DEPENDENCIES}
        OWN ${_generated_rs_files})
      set_property(GLOBAL APPEND PROPERTY NROS_ZEPHYR_CPP_FFI_RS ${_ffi_rs_pkg})
      get_property(_ffi_armed GLOBAL PROPERTY NROS_ZEPHYR_CPP_FFI_ARMED)
      if(NOT _ffi_armed)
        set_property(GLOBAL PROPERTY NROS_ZEPHYR_CPP_FFI_ARMED TRUE)
        cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
          CALL _nros_zephyr_cpp_ffi_image)
      endif()
    endif()

  else()
    # -- C path: .c sources + .h headers --

    # Issue 1857 — the predicted outputs, never a glob (see the C++ arm).
    # Measured: after phase-492 renamed the C files to rosidl's names, a glob
    # here compiled yesterday's `std_msgs_msg_bool.c` beside today's
    # `bool.c`, and the orphan failed the RFC-0090 version `#error`.
    set(_generated_sources ${_exp_src})
    set(_generated_headers ${_exp_hdr})

    if(NOT _generated_sources)
      message(FATAL_ERROR
        "nros-codegen produced no .c files for ${target} in ${_output_dir}")
    endif()

    target_sources(app PRIVATE ${_generated_sources})
    target_include_directories(app PRIVATE
      ${_output_dir}
      ${CMAKE_CURRENT_BINARY_DIR}/nano_ros_c
    )
  endif()

  # ---- phase-292 W2 (ASI wall #8) — Cyclone topic descriptors -------------
  # Moved into `_nros_zephyr_cyclonedds_typesupport` below (phase-531 W6), so a
  # generated RUST west application, which declares no interface package of its
  # own, reaches the same descriptors through
  # `nros_zephyr_interface_typesupport`.
  _nros_zephyr_cyclonedds_typesupport("${target}"
    INTERFACE_FILES ${_interface_files}
    DEPENDENCIES ${_ARG_DEPENDENCIES})
endfunction()

# _nros_zephyr_cpp_ffi_image()
#
# Issue 1645 — build and link the image's ONE message-FFI staticlib over the
# union every `nros_generate_interfaces(... LANGUAGE CPP)` call contributed
# (global property NROS_ZEPHYR_CPP_FFI_RS). Deferred to the end of the app
# directory by the first contributor, so it sees every package.
function(_nros_zephyr_cpp_ffi_image)
  set(_ffi_name "image")
  # Phase 140 — resolve templates/serdes directly from the
  # in-tree nano-ros checkout (the legacy install-local prefix is
  # gone). The Zephyr module ships under <repo>/zephyr/cmake/, so
  # walk up two dirs to reach the repo root.
  set(_nros_repo_dir "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/../..")
  get_filename_component(_nros_repo_dir "${_nros_repo_dir}" ABSOLUTE)

  set(_serdes_standalone_toml
      "${_nros_repo_dir}/packages/api/nros-cpp/cmake/nros-serdes-standalone-Cargo.toml")
  set(_template_dir
      "${_nros_repo_dir}/cmake")

  if(NOT EXISTS "${_serdes_standalone_toml}")
    message(FATAL_ERROR
      "nros-serdes standalone Cargo.toml not found at "
      "${_serdes_standalone_toml}. The nano-ros checkout looks incomplete.")
  endif()

  # Stage a proper crate directory for the per-FFI Cargo.toml's
  # `path = ` dependency. The upstream layout ships the
  # standalone Cargo.toml beside other cmake helpers under
  # `packages/api/nros-cpp/cmake/`, but Cargo needs the file
  # named `Cargo.toml` and the `src/` tree alongside it. Stage
  # both under the build dir on first configure (idempotent).
  set(_serdes_dir "${CMAKE_BINARY_DIR}/nros-rust/staged-nros-serdes")
  file(MAKE_DIRECTORY "${_serdes_dir}")
  configure_file(
    "${_serdes_standalone_toml}"
    "${_serdes_dir}/Cargo.toml"
    COPYONLY
  )
  # Stage the serdes crate source. Use a symlink so build.rs
  # reads always-fresh content without re-staging on every
  # configure; fall back to a directory copy if symlinks fail
  # (Windows + non-admin, etc.).
  if(NOT EXISTS "${_serdes_dir}/src")
    execute_process(
      COMMAND ${CMAKE_COMMAND} -E create_symlink
        "${_nros_repo_dir}/packages/core/nros-serdes/src"
        "${_serdes_dir}/src"
      RESULT_VARIABLE _serdes_link_rc
    )
    if(NOT _serdes_link_rc EQUAL 0)
      file(COPY "${_nros_repo_dir}/packages/core/nros-serdes/src"
        DESTINATION "${_serdes_dir}")
    endif()
  endif()

  # Set up temp Cargo project
  set(_ffi_crate_dir "${CMAKE_CURRENT_BINARY_DIR}/nano_ros_cpp_ffi_${_ffi_name}")
  set(_ffi_crate_src "${_ffi_crate_dir}/src")
  set(_ffi_target_dir "${_ffi_crate_dir}/target")

  file(MAKE_DIRECTORY "${_ffi_crate_src}")

  # Detect Rust target for cross-compilation
  nros_detect_rust_target()

  # phase-336 — the C++ FFI glue uses the `cpp-ffi-glue` CARVE-OUT, not the
  # ambient profile: it is a second Rust staticlib in a link that already
  # contains the nros-cpp archive, and at `lto = "off"` both carry std's
  # panicking codegen unit → `multiple definition of
  # __rustc::rust_begin_unwind`. Fat LTO internalizes it.
  nros_resolve_carve_out_profile(cpp-ffi-glue _NROS_FFI)
  set(_nros_cargo_profile "${_NROS_FFI_PROFILE}")
  set(_nros_cargo_profile_dir "${_NROS_FFI_DIR}")

  # phase-340 W3 — the triple is always named (host included), so the
  # artifact always sits under it. `_nros_ffi_cargo_args` rejects an empty
  # RUST_TARGET, so a missing `nros_detect_rust_target()` fails loudly
  # rather than writing to a path this line would not have predicted.
  set(_ffi_lib "${_ffi_target_dir}/${NROS_RUST_TARGET}/${_nros_cargo_profile_dir}/libnano_ros_cpp_ffi_${_ffi_name}.a")
  # issue 0820 — cargo writes its dep-info beside the artifact, same
  # basename, `.d`. Derived from `_ffi_lib` so the two cannot drift.
  string(REGEX REPLACE "\\.a$" ".d" _ffi_dep_file "${_ffi_lib}")

  # Generate Cargo.toml from template
  set(FFI_TARGET "${_ffi_name}")
  set(SERDES_DIR "${_serdes_dir}")

  configure_file(
    "${_template_dir}/cpp_ffi_Cargo.toml.in"
    "${_ffi_crate_dir}/Cargo.toml"
    @ONLY
  )
  # phase-336 — no appended `[profile.*]` mirror. The preset's definition
  # reaches this generated crate as `CARGO_PROFILE_*` environment variables
  # (NROS_CARGO_PROFILE_ENV, set on the cargo command below), which works
  # for EVERY nros-* profile rather than the one name this branch knew.

  # Generate lib.rs: the de-duplicated dep closure + own files, each
  # include!()d into one flat module scope. De-dup + emission live in the
  # shared core (Phase 246). The Zephyr path uses ABSOLUTE include paths
  # (its crate dir + generated outputs co-resolve in one binary tree).
  get_property(_ffi_rs_all GLOBAL PROPERTY NROS_ZEPHYR_CPP_FFI_RS)
  list(REMOVE_DUPLICATES _ffi_rs_all)
  _nros_write_ffi_lib_rs(
    CRATE_SRC "${_ffi_crate_src}"
    TEMPLATE "${_template_dir}/ffi_lib_rs.in"
    RS_FILES ${_ffi_rs_all}
    PATH_MODE absolute)

  # Tier-2/3 embedded targets (e.g. armv7a-none-eabi for cortex_a9)
  # need rustup to know which toolchain + target combo to use. The
  # example tree's rust-toolchain.toml isn't visible from this
  # build dir, so drop a copy alongside the FFI Cargo.toml. For
  # the host targets (x86_64 / i686), no override is needed. Issue 2312: the
  # channel is the pinned nightly read from its one source, and the
  # components and target are what `nros setup board` installs on it.
  nros_rust_target_builds_std("${NROS_RUST_TARGET}" _nros_builds_std)
  if(_nros_builds_std)
    nros_pinned_nightly(_nros_nightly)
    file(WRITE "${_ffi_crate_dir}/rust-toolchain.toml"
"# Auto-generated by nros_generate_interfaces.cmake — the pinned nightly
# (config/rust-toolchains.txt, issue 2312), the one nros-c builds with.
[toolchain]
channel = \"${_nros_nightly}\"
components = [\"rust-src\", \"rustfmt\"]
targets = [\"${NROS_RUST_TARGET}\"]
")
  endif()

  # Build the FFI staticlib
  # Assemble cargo args via the shared core (Phase 246.3). Tier-2/3 embedded
  # triples ship no precompiled std → build core+alloc from rust-src (inline
  # -Z, not a .cargo/config.toml). Toolchain pin lives in rust-toolchain.toml
  # written above. A host board still names its triple (phase-340 W3); only
  # `-Z build-std` is conditional, on the embedded triples that ship no std.
  set(_zephyr_build_std "")
  if(_nros_builds_std)
    set(_zephyr_build_std "core,alloc,compiler_builtins")
  endif()
  _nros_ffi_cargo_args(_cargo_ffi_args
    MANIFEST "${_ffi_crate_dir}/Cargo.toml"
    TARGET_DIR "${_ffi_target_dir}"
    PROFILE "${_nros_cargo_profile}"
    RUST_TARGET "${NROS_RUST_TARGET}"
    BUILD_STD "${_zephyr_build_std}")

  # phase-336 — carry the preset definition; this generated crate is its own
  # workspace root and no longer appends a `[profile.*]` block.
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
  # issue 0820 — the sibling of the generic generator's block, and the
  # narrower of the two: DEPENDS here names only the crate's manifest and
  # `lib.rs`, so neither the GENERATED message sources nor any nano-ros
  # Rust crate had an edge on this archive. The DEPFILE covers both, from
  # the graph cargo already computes.
  # Issue 1304 — see cmake/NanoRosRustTool.cmake (via NanoRosCodegenCore).
  nros_rust_tool(_ffi_cargo cargo)
  # Issue 2092 — this archive is whole-archived BESIDE the runtime's
  # `libnros_cpp.a`, and both carry compiler_builtins' 34 C-compiled
  # intrinsics as strong globals. Keep the FFI surface strong and weaken the
  # rest, or the first intrinsic the image's C calls (Zephyr's `bitarray.c`
  # calls `__popcountdi2`) is defined twice. See NanoRosSecondStaticlib.cmake.
  nros_second_staticlib_weaken_command(_ffi_weaken
    ARCHIVE "${_ffi_lib}" KEEP "nros_cpp_*")
  add_custom_command(
    OUTPUT "${_ffi_lib}"
    COMMAND ${_ffi_env} "${_ffi_cargo}" ${_cargo_ffi_args}
    COMMAND ${_ffi_weaken}
    DEPENDS "${_ffi_crate_dir}/Cargo.toml" "${_ffi_crate_src}/lib.rs"
    DEPFILE "${_ffi_dep_file}"
    WORKING_DIRECTORY "${_ffi_crate_dir}"
    COMMENT "Building Rust FFI glue for ${_ffi_name} C++ bindings"
    VERBATIM
  )

  # Custom target carrying the .a build edge for `app`.
  add_custom_target(nros_cpp_ffi_${_ffi_name}_build DEPENDS "${_ffi_lib}")

  # Link FFI staticlib to app, WHOLE-ARCHIVED. The generated message C++
  # headers call these `nros_cpp_{serialize,deserialize,publish}_*` FFI
  # symbols from inline functions compiled into the app objects AND into
  # any component library (nano_ros_node_register) — all of which may sit
  # AFTER this `.a` on the final link line. GNU ld processes left→right and
  # discards `.a` members whose symbols aren't yet referenced, so a plain
  # link drops them → "undefined reference to nros_cpp_deserialize_*". The
  # FFI glue is small (per-message ser/de/publish), so whole-archiving is
  # the order-independent fix (issue 0056). CMake 3.24's
  # $<LINK_LIBRARY:WHOLE_ARCHIVE> isn't available on the Zephyr-pinned CMake
  # (3.22) — use raw flags and an explicit build-order dependency on `app`.
  #
  # #192 (the #193 class) — the flags MUST be ONE comma-joined `-Wl,` item,
  # not three separate items: CMake < 3.24 de-duplicates repeated identical
  # flag items across the aggregated link line, so with two generated
  # packages (std_msgs + builtin_interfaces) the second triple's
  # `-Wl,--whole-archive` / `-Wl,--no-whole-archive` collide with the
  # first's AND with Zephyr's own whole-archive bracket — the surviving
  # tokens leave an UNCLOSED bracket that swallows everything to the end of
  # the link line, including picolibc's `-lc` (every `libc_ssp_*` member
  # force-included → `__stack_chk_init` → undefined `getentropy` on targets
  # with no entropy driver, e.g. FVP AEMv8-R). The comma-joined form is a
  # single unique-per-lib token the de-dup cannot split.
  #
  # Phase 246.4 — the link wiring is intentionally NOT shared with the
  # canonical generator: it solves the OPPOSITE ld-order problem (there, the
  # nros-cpp runtime must follow the ffi lib; here, the ffi lib must follow
  # the app/component objects) for a different target model (Zephyr `app` vs
  # a per-package INTERFACE library). The IMPORTED-target dance the canonical
  # path needs is dead weight here, so the FFI lib is linked by raw path.
  target_link_libraries(app PRIVATE
    "-Wl,--whole-archive,${_ffi_lib},--no-whole-archive")
  add_dependencies(app nros_cpp_ffi_${_ffi_name}_build)
endfunction()
