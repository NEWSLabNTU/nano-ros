# Cyclone DDS per-message type support for a Zephyr image (phase-531 W6).
#
# Split out of `nros_generate_interfaces.cmake` so a RUST application can reach
# it. That file is included only on the C and C++ API paths, because what it
# mostly does is generate C/C++ message bindings. The descriptors are a different
# fact: the Zephyr port links the C++ `nros_rmw_cyclonedds`, whose topic lookup is
# an idlc descriptor table, whatever language the nodes are written in. So this
# file is included for every image (`zephyr/CMakeLists.txt`), and both callers
# share the one body:
#
#   * `nros_generate_interfaces` (C/C++, reached through `find_package(<pkg>)`);
#   * `nros_zephyr_interface_typesupport` (a generated Rust west application,
#     which calls no find_package; `nros build` names its interface closure).
#
# Both are no-ops unless `CONFIG_NROS_RMW_CYCLONEDDS`.

# _nros_zephyr_cyclonedds_typesupport(<pkg> INTERFACE_FILES <files…> DEPENDENCIES <pkgs…>)
#
# phase-292 W2 (ASI wall #8) — Cyclone topic descriptors for ONE interface
# package, compiled into `<pkg>__cyclonedds_ts` and whole-archived into `app`.
# A no-op unless `CONFIG_NROS_RMW_CYCLONEDDS`. Called by
# `nros_generate_interfaces` (the C/C++ road, through `find_package(<pkg>)`) and
# by `nros_zephyr_interface_typesupport` (a generated Rust application). The
# body is unchanged from when it sat inline in the former.
function(_nros_zephyr_cyclonedds_typesupport target)
  cmake_parse_arguments(_ARG "" "" "INTERFACE_FILES;DEPENDENCIES" ${ARGN})
  set(_interface_files ${_ARG_INTERFACE_FILES})
  # The canonical workspace generator (cmake/NanoRosGenerateInterfaces.cmake,
  # phase-171.C.runtime) emits + whole-archives an idlc descriptor/register
  # lib per package; this Zephyr-module variant never did, so every
  # find_descriptor() on a module-generated type failed at runtime
  # (create_subscription/-publisher → TransportError). Same generation, app
  # target model: descriptors compile into a per-package static lib linked
  # whole-archive into `app` (comma-joined single -Wl token — the #192 CMake
  # de-dup hazard applies here identically).
  # issue 1673 — this road ARMS ITSELF. It was guarded on
  # `COMMAND nros_rmw_cyclonedds_generate_from_msg`, which exists only once
  # `NrosRmwCycloneddsTypeSupport` has been included — and the only thing that
  # included it was the EXAMPLE's own hand-written descriptor block, run before
  # its `find_package(<pkg>)`. So this road fired only in images that ALSO
  # generated the same descriptors by hand, and compiled every one of them a
  # second time. That duplication was hidden by a global
  # `--allow-multiple-definition` until issue 1636 scoped the flag to C++ FFI
  # images, after which every Zephyr C + Cyclone DDS image failed to link on a
  # duplicate `*__desc` / `register_*`.
  #
  # And the guard failed OPEN in the other direction: an image that
  # find_package'd an interface package without hand-including the module got
  # NO descriptors and no error, i.e. `find_descriptor()` failing at runtime.
  # Arming here makes the two cases one: the module is included by full path
  # (its dir is a cache var the cyclone backend exports, so no
  # CMAKE_MODULE_PATH scoping is involved), and an absent module is a
  # configure error rather than a silent skip.
  if(CONFIG_NROS_RMW_CYCLONEDDS AND NOT COMMAND nros_rmw_cyclonedds_generate_from_msg)
    if(NOT NROS_CYCLONE_CMAKE_DIR
       OR NOT EXISTS "${NROS_CYCLONE_CMAKE_DIR}/NrosRmwCycloneddsTypeSupport.cmake")
      # issue 1673.
      message(FATAL_ERROR
        "nros_generate_interfaces: CONFIG_NROS_RMW_CYCLONEDDS is set but the "
        "Cyclone DDS type-support module was not found "
        "(NROS_CYCLONE_CMAKE_DIR='${NROS_CYCLONE_CMAKE_DIR}'). Without it this "
        "image would get no topic descriptors and fail at runtime in "
        "find_descriptor(). The nros Zephyr module points NROS_CYCLONE_CMAKE_DIR "
        "at packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake in its nano-ros "
        "checkout: restore that directory, or unset CONFIG_NROS_RMW_CYCLONEDDS.")
    endif()
    set(IDLC_EXECUTABLE "${NROS_CYCLONE_IDLC}"
        CACHE FILEPATH "Host Cyclone DDS idlc for descriptor generation" FORCE)
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${NROS_CYCLONE_SCRIPTS_DIR}")
    include("${NROS_CYCLONE_CMAKE_DIR}/NrosRmwCycloneddsTypeSupport.cmake")
  endif()
  # Issue 2159 -- ONCE per package, not once per (package, language). The
  # guard above is language-keyed, because the C and C++ bindings are distinct
  # outputs; the Cyclone descriptors are not -- they are C for both, built into
  # one `<pkg>__cyclonedds_ts` that `app` whole-archives. An image needing both
  # languages (a C leaf whose TYPED carrier is C++) ran this twice and the
  # second `add_library` failed: "cannot create target
  # builtin_interfaces__cyclonedds_ts because another target with the same
  # name already exists". The first pass already linked it into `app`, so a
  # later pass has nothing to add.
  get_property(_nros_cyc_done GLOBAL PROPERTY "_NROS_CYC_TS_GENERATED_${target}")
  if(CONFIG_NROS_RMW_CYCLONEDDS AND NOT _nros_cyc_done)
    set_property(GLOBAL PROPERTY "_NROS_CYC_TS_GENERATED_${target}" TRUE)
    set(_cyc_ifaces "")
    foreach(_if ${_interface_files})
      if(_if MATCHES "\\.(msg|srv|action)$")
        file(READ "${_if}" _if_body)
        if(_if_body MATCHES "(\n|^)[ \t]*wstring[ \t<\\[]")
          message(STATUS
            "nros_generate_interfaces(${target}): skipping cyclonedds "
            "descriptor for ${_if} — `wstring` is unsupported by the "
            "bundled Cyclone DDS 0.10.5 idlc.")
        else()
          list(APPEND _cyc_ifaces "${_if}")
        endif()
      endif()
    endforeach()
    if(_cyc_ifaces)
      list(GET _cyc_ifaces 0 _cyc_first)
      get_filename_component(_cyc_ifdir "${_cyc_first}" DIRECTORY)
      get_filename_component(_cyc_pkgdir "${_cyc_ifdir}" DIRECTORY)
      set(_cyc_idl_root "${CMAKE_BINARY_DIR}/cyclonedds-ts/_idlroot")
      set(_cyc_gen_root "${CMAKE_BINARY_DIR}/cyclonedds-ts/_genroot")
      # phase-306 W2 (issue 0258) — file-level cross-package IDL ordering;
      # twin of the canonical generator's cyclone stage (keep in sync).
      # Deps' generated .idl closures come from the CACHE stash written by
      # whichever call generated them; every pass-2 idlc command then
      # depends on the dep IDL FILES, not just the sibling ts target.
      set(_cyc_dep_idls "")
      foreach(_dep ${_ARG_DEPENDENCIES})
        if(DEFINED CACHE{_NROS_PKG_${_dep}_CYC_IDL_FILES})
          list(APPEND _cyc_dep_idls "$CACHE{_NROS_PKG_${_dep}_CYC_IDL_FILES}")
        endif()
      endforeach()
      if(_cyc_dep_idls)
        list(REMOVE_DUPLICATES _cyc_dep_idls)
      endif()
      nros_rmw_cyclonedds_generate_from_msg(_cyc_sources
        PKG_NAME   "${target}"
        PKG_DIR    "${_cyc_pkgdir}"
        INTERFACES ${_cyc_ifaces}
        INCLUDE_ROOT "${_cyc_idl_root}"
        GEN_ROOT     "${_cyc_gen_root}"
        OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-ts/${target}"
        IDL_DEPENDS ${_cyc_dep_idls}
        IDL_FILES_VAR _cyc_own_idls)
      # Stash this package's IDL closure for later-generated dependents
      # (persists across directory scopes / guard-skipped calls).
      set(_cyc_stash "${_cyc_dep_idls}")
      list(APPEND _cyc_stash ${_cyc_own_idls})
      if(_cyc_stash)
        list(REMOVE_DUPLICATES _cyc_stash)
      endif()
      set(_NROS_PKG_${target}_CYC_IDL_FILES "${_cyc_stash}" CACHE INTERNAL
        "phase-306 W2: ${target}'s generated cyclone .idl closure")
      if(_cyc_sources)
        add_library(${target}__cyclonedds_ts STATIC ${_cyc_sources})
        target_include_directories(${target}__cyclonedds_ts PRIVATE
          "${_cyc_gen_root}")
        # Zephyr flags/includes (dds/dds.h via the module's cyclone dirs).
        target_link_libraries(${target}__cyclonedds_ts PRIVATE zephyr_interface)
        # Cross-package IDL include ordering (deps populate the shared root
        # before this package's idlc runs).
        foreach(_dep ${_ARG_DEPENDENCIES})
          if(TARGET ${_dep}__cyclonedds_ts)
            add_dependencies(${target}__cyclonedds_ts ${_dep}__cyclonedds_ts)
          endif()
        endforeach()
        # Same idiom as the FFI staticlib above: ONE comma-joined `-Wl`
        # link ITEM by literal archive path. Link OPTIONS on the static
        # `app` lib never reach the final zephyr link; a genex inside a
        # link-libraries string trips $<LINK_ONLY>'s comma parsing; and
        # the #192 de-dup hazard forbids separate flag tokens.
        set(_cyc_ts_archive
          "${CMAKE_CURRENT_BINARY_DIR}/lib${target}__cyclonedds_ts.a")
        set_target_properties(${target}__cyclonedds_ts PROPERTIES
          ARCHIVE_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}"
          OUTPUT_NAME "${target}__cyclonedds_ts")
        target_link_libraries(app PRIVATE
          "-Wl,--whole-archive,${_cyc_ts_archive},--no-whole-archive")
        add_dependencies(app ${target}__cyclonedds_ts)
      endif()
    endif()
  endif()
endfunction()

# nros_zephyr_interface_typesupport(PACKAGES <spec>…)
#
# phase-531 W6 — the per-message type support a generated RUST west
# application needs on a backend that has a per-message hook and no rlib (Cyclone
# DDS: the Zephyr port links the C++ `nros_rmw_cyclonedds`, which looks a topic
# type up in its idlc descriptor table). A C/C++ image gets these through
# `find_package(<pkg>)` -> `nros_generate_interfaces`. A Rust image calls neither,
# so before this its descriptors came only from a hand-written block in the leaf's
# `CMakeLists.txt`, and a generated application had none: `find_descriptor()`
# failed at runtime.
#
# `nros build` emits this call with the image's interface closure in DEPENDENCY
# ORDER (a package after everything it depends on), because idlc needs a
# dependency's IDL on the shared include root first. A `<spec>` is `<pkg>` (found
# under `AMENT_PREFIX_PATH`, as `nros_generate_interfaces` falls back to) or
# `<pkg>=<dir>` (a workspace interface package; a relative `<dir>` is read
# against the calling application's source directory). A package with no interface
# files is skipped. No interface library is generated: a Rust image's
# messages are its generated crates. Only the descriptors are made.
function(nros_zephyr_interface_typesupport)
  cmake_parse_arguments(_ARG "" "" "PACKAGES" ${ARGN})
  if(NOT CONFIG_NROS_RMW_CYCLONEDDS)
    return()
  endif()
  set(_prior "")
  foreach(_spec ${_ARG_PACKAGES})
    if(_spec MATCHES "^([^=]+)=(.+)$")
      set(_pkg "${CMAKE_MATCH_1}")
      set(_dirs "${CMAKE_MATCH_2}")
      if(NOT IS_ABSOLUTE "${_dirs}")
        # Relative to the generated application, so its file is byte-identical
        # across machines.
        get_filename_component(_dirs "${CMAKE_CURRENT_SOURCE_DIR}/${_dirs}" ABSOLUTE)
      endif()
    else()
      set(_pkg "${_spec}")
      set(_dirs "")
      if(DEFINED ENV{AMENT_PREFIX_PATH})
        string(REPLACE ":" ";" _ament_paths "$ENV{AMENT_PREFIX_PATH}")
        foreach(_prefix ${_ament_paths})
          if(IS_DIRECTORY "${_prefix}/share/${_pkg}")
            set(_dirs "${_prefix}/share/${_pkg}")
            break()
          endif()
        endforeach()
      endif()
    endif()
    set(_files "")
    if(_dirs)
      file(GLOB _msg CONFIGURE_DEPENDS "${_dirs}/msg/*.msg")
      file(GLOB _srv CONFIGURE_DEPENDS "${_dirs}/srv/*.srv")
      file(GLOB _act CONFIGURE_DEPENDS "${_dirs}/action/*.action")
      list(APPEND _files ${_msg} ${_srv} ${_act})
    endif()
    if(NOT _files)
      message(STATUS
        "nros_zephyr_interface_typesupport: ${_pkg} has no interface files "
        "(looked in '${_dirs}'), so it gets no Cyclone descriptors")
      continue()
    endif()
    _nros_zephyr_cyclonedds_typesupport("${_pkg}"
      INTERFACE_FILES ${_files}
      DEPENDENCIES ${_prior})
    list(APPEND _prior "${_pkg}")
  endforeach()
endfunction()
