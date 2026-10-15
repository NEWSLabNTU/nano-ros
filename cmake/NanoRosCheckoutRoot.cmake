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
