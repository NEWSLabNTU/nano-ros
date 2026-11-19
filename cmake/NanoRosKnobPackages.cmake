# The out-of-tree packages' knob rows (phase-581 W3, RFC-0140 D5).
#
# `cmake/NanoRosKnobRegistry.cmake` is GENERATED from the in-tree registry and
# committed; a package outside the tree cannot edit it. `nros configure`
# writes the out-of-tree rows in the same variables to
# `<build>/nros/knobs/registry.cmake`, beside the merged catalog it names to
# the configure as `-DNROS_KNOB_CATALOG=<path>`, and this file includes it
# right after the committed one.
#
# Included at FILE scope, after `NanoRosKnobRegistry.cmake` (an include()
# inside a function frame drops the file's variables when the frame pops). A
# configure `nros` did not run (no `NROS_KNOB_CATALOG`) has no package rows.

if(DEFINED NROS_KNOB_CATALOG AND NOT NROS_KNOB_CATALOG STREQUAL "")
    get_filename_component(_nros_knob_pkg_dir "${NROS_KNOB_CATALOG}" DIRECTORY)
    if(EXISTS "${_nros_knob_pkg_dir}/registry.cmake")
        include("${_nros_knob_pkg_dir}/registry.cmake")
        # A configure re-runs when the package rows change.
        set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
            "${_nros_knob_pkg_dir}/registry.cmake")
    endif()
    unset(_nros_knob_pkg_dir)
endif()
