# NanoRosCheckoutRoot.cmake — which nano-ros checkout does a path belong to?
#
# The cmake spelling of the ONE checkout-marker walk (RFC-0103 D6):
# `nros_build_paths::checkout_root_of` in Rust, `nros_checkout_root` in
# `scripts/lib/checkout-paths.sh`. `check-inherited-checkout-paths.py` pins the
# three marker spellings to each other.
#
# Its own module, with no side effects, because two consumers ask it:
# `NanoRosWorkspace.cmake` (which checkout encloses a workspace) and
# `NanoRosSharedCargoDir.cmake` (issue 1904: does THIS checkout own the shared
# cargo root it was handed). Including NanoRosWorkspace.cmake from the second
# would drag in its retired-name refusal and ROS-edition logic, which the Zephyr
# lane and the key probe (`scripts/lib/shared-cargo-key-probe.cmake`) must not
# run.
include_guard(GLOBAL)

# _nros_find_root(<start> <out_var>)
#
# Walk up from <start> to the first directory carrying the checkout marker — the
# DEEPEST enclosing checkout, which is what separates a worktree nested under
# `<main>/.claude/worktrees/` from `<main>` (issue 1391). Writes the path to
# <out_var> (PARENT_SCOPE), or `_NROS_ROOT-NOTFOUND` when nothing matches.
function(_nros_find_root start out_var)
    set(_dir "${start}")
    set(_max_walk 16)            # bounded — never walk past `/`
    while(_max_walk GREATER 0)
        # The ONE root marker (RFC-0103 D6) — `nros_build_paths::CHECKOUT_MARKER`.
        if(EXISTS "${_dir}/packages/core/nros-core/Cargo.toml")
            set(${out_var} "${_dir}" PARENT_SCOPE)
            return()
        endif()
        get_filename_component(_parent "${_dir}" DIRECTORY)
        if(_parent STREQUAL _dir)
            break()             # reached `/`
        endif()
        set(_dir "${_parent}")
        math(EXPR _max_walk "${_max_walk} - 1")
    endwhile()
    set(${out_var} "_NROS_ROOT-NOTFOUND" PARENT_SCOPE)
endfunction()

# _nros_checkout_scope_name(<here> <out_var>)
#
# Issues 1904/1921 — `<basename>-<sha256(realpath)[:12]>`, the name checkout
# <here> carries under a build root it shares. The cmake twin of
# `nros_checkout_scope_name` (scripts/lib/checkout-paths.sh) and
# `nros_build_paths::checkout_scope_name`.
function(_nros_checkout_scope_name here out_var)
    get_filename_component(_real "${here}" REALPATH)
    get_filename_component(_name "${_real}" NAME)
    string(SHA256 _hash "${_real}")
    string(SUBSTRING "${_hash}" 0 12 _hash)
    set(${out_var} "${_name}-${_hash}" PARENT_SCOPE)
endfunction()

# nros_checkout_scoped_root(<out_var> <root> [<here>])
#
# Issue 1921 — the ONE checkout-ownership rule for a build root, cmake spelling
# of `nros_checkout_scoped_root` (scripts/lib/checkout-paths.sh, where the rule
# is documented) and `nros_build_paths::checkout_scoped_root`. <root> when
# checkout <here> owns it — its deepest marker-walk owner is <here> after
# resolving symlinks, or it already lies inside <here>'s own
# `checkouts/<name>` scope — else `<root>/checkouts/<name>`. <here> defaults
# to the checkout this module belongs to.
#
# The idempotent arm is what lets every in-repo producer pass an ALREADY-scoped
# path (`nros_build_dir corrosion-cargo …` from the shell,
# `shared_probe_dir(…)` from the CLI) and get it back unchanged, while a raw
# `-DNROS_SHARED_CARGO_ROOT=/fast/x` a user passes by hand is scoped here.
function(nros_checkout_scoped_root out_var root)
    if(ARGC GREATER 2)
        set(_here_in "${ARGV2}")
    else()
        set(_here_in "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/..")
    endif()
    get_filename_component(_here "${_here_in}" REALPATH)
    _nros_checkout_scope_name("${_here}" _name)
    get_filename_component(_root "${root}" ABSOLUTE)
    _nros_find_root("${_root}" _owner)
    if(_owner)
        get_filename_component(_owner "${_owner}" REALPATH)
        if(_owner STREQUAL _here)
            set(${out_var} "${_root}" PARENT_SCOPE)
            return()
        endif()
    endif()
    string(FIND "${_root}/" "/checkouts/${_name}/" _pos)
    if(_pos GREATER -1)
        set(${out_var} "${_root}" PARENT_SCOPE)
        return()
    endif()
    set(${out_var} "${_root}/checkouts/${_name}" PARENT_SCOPE)
endfunction()

# nros_reroot_cached_checkout_paths(<here>)
#
# Issue 2931 — issue 1280's rule, applied to the CMake CACHE. Every
# `set(<var> <default> CACHE PATH|FILEPATH ...)` whose default is derived from
# the tree (`${CMAKE_CURRENT_LIST_DIR}/../nros-platform-api/include`, a board's
# config dir, ...) is written ONCE, on the first configure of a build dir, and
# a later configure against ANOTHER nano-ros root keeps it. After `nros pin`
# moves an installed project from one SDK to the next, the old SDK is still in
# the store, so the cached path still exists: nothing refuses it, and the new
# SDK's sources compile against the old SDK's headers (the nros11 -> nros12
# upgrade: `platform.c: unknown type name 'nros_wall_clock_state_t'`).
#
# So, before any project reads one: a cached value (any type but STATIC and
# BOOL) that is one absolute path whose owning nano-ros root (deepest marker walk) is a DIFFERENT root than <here> is
# re-rooted onto <here>, same sub-path, and said once. A value outside every
# nano-ros root (a toolchain, an SDK in the store, a user's own tree), one
# already inside <here>, and one inside the consuming project's own source or
# build tree are kept. Only when <here> is itself a root: there is
# nothing to re-root onto otherwise. The shell spelling of the rule is
# `scripts/lib/checkout-paths.sh`.
function(nros_reroot_cached_checkout_paths here)
    get_filename_component(_here "${here}" REALPATH)
    _nros_find_root("${_here}" _here_root)
    if(NOT _here_root)
        return()
    endif()
    get_filename_component(_here_root "${_here_root}" REALPATH)
    if(NOT _here_root STREQUAL _here)
        return()
    endif()
    get_cmake_property(_vars CACHE_VARIABLES)
    set(_moved "")
    foreach(_v IN LISTS _vars)
        # Every type that can hold a path. INTERNAL included: a write-once
        # guard (`if(NOT DEFINED CACHE{_NANO_ROS_PREFIX})`) keeps an INTERNAL
        # entry exactly as it keeps a PATH one, and the nros11 -> nros12 move
        # left `_NANO_ROS_PREFIX` on nros11 that way. STATIC is CMake's own.
        get_property(_type CACHE "${_v}" PROPERTY TYPE)
        if(_type STREQUAL "STATIC" OR _type STREQUAL "BOOL")
            continue()
        endif()
        set(_value "$CACHE{${_v}}")
        if(_value STREQUAL "" OR NOT IS_ABSOLUTE "${_value}" OR _value MATCHES ";")
            continue()
        endif()
        # The consuming project's own source and build trees are its paths,
        # even when that project sits inside another checkout (an in-repo
        # example configured by a worktree's nano-ros).
        set(_own FALSE)
        foreach(_d IN ITEMS "${CMAKE_SOURCE_DIR}" "${CMAKE_BINARY_DIR}")
            cmake_path(IS_PREFIX _d "${_value}" NORMALIZE _in)
            if(_in)
                set(_own TRUE)
            endif()
        endforeach()
        if(_own)
            continue()
        endif()
        _nros_find_root("${_value}" _owner)
        if(NOT _owner)
            continue()
        endif()
        get_filename_component(_owner_real "${_owner}" REALPATH)
        if(_owner_real STREQUAL _here)
            continue()
        endif()
        file(RELATIVE_PATH _rel "${_owner}" "${_value}")
        if(_rel STREQUAL "")
            set(_new "${_here}")
        else()
            set(_new "${_here}/${_rel}")
        endif()
        get_property(_doc CACHE "${_v}" PROPERTY HELPSTRING)
        set(${_v} "${_new}" CACHE ${_type} "${_doc}" FORCE)
        list(APPEND _moved "${_v}")
    endforeach()
    if(_moved)
        list(LENGTH _moved _n)
        list(JOIN _moved ", " _names)
        message(STATUS
            "nano-ros: ${_n} cached path(s) named another nano-ros root and were "
            "re-rooted onto ${_here}: ${_names}")
    endif()
endfunction()
