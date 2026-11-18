# shellcheck shell=bash
# Issue 2622 — where nano-ros keeps ITS OWN state for a NuttX tree.
#
# `build-nuttx.sh`'s lock, its `HEAD:sha256(defconfig)` markers and the
# per-config export snapshots live in `<tree>.nros-build/`, a SIBLING of the
# NuttX tree, never inside it: the tree is a vendored submodule (or a store
# copy), and files nano-ros wrote there read as unsaved work in every checkout
# that built NuttX. Beside the tree, not under a checkout's build root, because
# what they describe is that tree's in-tree kernel build, which every checkout
# sharing the tree shares.
#
# The shell spelling of `nros_build_paths::nuttx_state_dir` /
# `nuttx_export_snapshot` (Rust) and `nros_nuttx_export_snapshot` (cmake);
# `check-nuttx-state-outside-tree` holds the spellings together.

# nros_nuttx_state_dir <nuttx tree>
nros_nuttx_state_dir() {
    local tree="${1%/}"
    printf '%s.nros-build\n' "$tree"
}

# nros_nuttx_export_snapshot <nuttx tree> <config id>
nros_nuttx_export_snapshot() {
    printf '%s/nros-nuttx-export-%s\n' "$(nros_nuttx_state_dir "$1")" "$2"
}

# nros_nuttx_retire_in_tree_state <nuttx tree>
#
# Remove the state earlier builds wrote INSIDE the tree. Exact names only, and
# only paths git does not track there; NuttX's own build output is not touched.
nros_nuttx_retire_in_tree_state() {
    local tree="${1%/}" p
    for p in "$tree"/.nros-nuttx-build-head* "$tree"/.nros-nuttx-build.lock \
        "$tree"/nros-nuttx-export-*; do
        [ -e "$p" ] || continue
        if git -C "$tree" ls-files --error-unmatch -- "${p#"$tree"/}" >/dev/null 2>&1; then
            continue
        fi
        rm -rf -- "$p"
    done
}
