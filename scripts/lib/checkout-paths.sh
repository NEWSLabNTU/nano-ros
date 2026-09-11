# shellcheck shell=bash
# An inherited absolute path that names ANOTHER nano-ros checkout — issue 1280.
#
# ONE rule, stated here and mirrored in exactly two other places because the
# three build systems cannot call each other (the `riscv64` precedent in
# `nros-build-paths`): `nros_build_paths::reroot_foreign` for cargo build
# scripts, and `just/sdk-env.just`'s prefix rewrite for `just` recipes.
# `check-inherited-checkout-paths.py` pins all three to this file.
#
# ## The rule
#
# A path-valued variable is resolved ENV-FIRST everywhere here, and that order
# is deliberate: it is how an out-of-tree SDK gets used. But a linked git
# worktree inherits its parent shell's environment, and every one of those
# values is an ABSOLUTE path into the checkout that shell was activated in —
# so env-first hands a worktree build the OTHER checkout's FreeRTOS, ThreadX,
# NuttX, platform sources, public headers and `build/` tree, and the worktree's
# own edits are never compiled. Agent sessions work in worktrees by default
# here, so this is the common case, not an edge one.
#
# The discriminator is therefore NOT "is the variable set". It is where the
# value points:
#
#   outside any nano-ros checkout  -> KEEP IT. A real out-of-tree SDK; this is
#                                     what env-first exists for.
#   inside THIS checkout           -> KEEP IT. Nothing to decide.
#   inside a DIFFERENT checkout    -> RE-ROOT IT here, and say so. Both trees
#                                     are nano-ros checkouts, so the relative
#                                     sub-path is identical by construction.
#
# ## Why the marker and not `.git`
#
# A linked worktree's `.git` is a FILE, not a directory (issue 1336), and
# `git rev-parse` answers about the CALLER's repository rather than about an
# arbitrary path. The question here is lexical — *which checkout does this
# spelling name* — so it is answered by walking up for the tree's one
# checkout marker, exactly as `nros_launcher::checkout::MONOREPO_MARKER` and
# `check-zephyr-workspace-checkout.sh`'s `owner_of` already do.
#
# Sourced from `bash`, `zsh` and plain POSIX `sh` (activate.sh reaches it
# through `scripts/sdk-env.sh`), so: no arrays, no `[[ ]]`, no `${BASH_SOURCE}`
# on an executed line.

# The file whose presence marks a nano-ros source tree. Same string as
# `nros_launcher::checkout::MONOREPO_MARKER` and the `CHECKOUT_MARKER` in
# `nros-build-paths`; the gate refuses a fourth spelling.
NROS_CHECKOUT_MARKER="packages/core/nros-core/Cargo.toml"

# nros_checkout_root <path>
#
# Print the nano-ros checkout `<path>` belongs to, or nothing when it belongs
# to none. Purely lexical: the path need not exist (an unprovisioned SDK dir is
# still attributable) and may name a file (`NROS_ESP_IDF_ENV_SHIM`).
#
# A RELATIVE path answers nothing on purpose — it cannot have been inherited
# from another checkout, because it is resolved against the caller's own cwd.
nros_checkout_root() {
    # `${1:-}`, not `$1`: this file is sourced into `set -u` scripts
    # (`build-root.sh`'s callers), where a missing argument is a hard error
    # rather than the empty answer every caller here treats as "no rewrite".
    _nros_cp_dir="${1:-}"
    case "$_nros_cp_dir" in
        /*) ;;
        *)
            unset _nros_cp_dir
            return 0
            ;;
    esac
    # `${d%/*}` rather than `$(dirname "$d")`: this walk runs once per
    # environment variable on every `just` invocation, and a subprocess per
    # ancestor made that 84 ms. Parameter expansion makes it ~3 ms.
    while [ -n "$_nros_cp_dir" ] && [ "$_nros_cp_dir" != "/" ]; do
        if [ -f "$_nros_cp_dir/$NROS_CHECKOUT_MARKER" ]; then
            printf '%s' "$_nros_cp_dir"
            unset _nros_cp_dir
            return 0
        fi
        _nros_cp_dir="${_nros_cp_dir%/*}"
    done
    unset _nros_cp_dir
    return 0
}

# nros_reroot_checkout_path <value> <here>
#
# The rule above, applied to one value. Always prints something: the re-rooted
# path when `<value>` belongs to a checkout other than `<here>`, and `<value>`
# unchanged in every other case. Never fails — a caller with no usable `<here>`
# (an out-of-tree consumer) keeps what it was given.
nros_reroot_checkout_path() {
    _nros_cp_value="${1:-}"
    _nros_cp_here="${2:-}"

    if [ -z "$_nros_cp_value" ] || [ -z "$_nros_cp_here" ]; then
        printf '%s' "$_nros_cp_value"
        unset _nros_cp_value _nros_cp_here
        return 0
    fi

    _nros_cp_owner="$(nros_checkout_root "$_nros_cp_value")"
    if [ -z "$_nros_cp_owner" ]; then
        # Outside any nano-ros checkout — a real out-of-tree SDK. KEEP.
        printf '%s' "$_nros_cp_value"
        unset _nros_cp_value _nros_cp_here _nros_cp_owner
        return 0
    fi

    # `pwd -P` on both sides: a checkout reached through a symlinked parent is
    # the same tree under a second name, and comparing the spellings would call
    # it foreign (issue 0375's two-names-for-one-tree, the reason
    # `check-zephyr-workspace-checkout.sh` resolves both sides too).
    _nros_cp_here_real="$(cd "$_nros_cp_here" 2>/dev/null && pwd -P)" || _nros_cp_here_real=""
    _nros_cp_owner_real="$(cd "$_nros_cp_owner" 2>/dev/null && pwd -P)" || _nros_cp_owner_real=""
    if [ -z "$_nros_cp_here_real" ] || [ "$_nros_cp_owner_real" = "$_nros_cp_here_real" ]; then
        printf '%s' "$_nros_cp_value"
        unset _nros_cp_value _nros_cp_here _nros_cp_owner \
            _nros_cp_here_real _nros_cp_owner_real
        return 0
    fi

    _nros_cp_rel="${_nros_cp_value#"$_nros_cp_owner"}"
    _nros_cp_rel="${_nros_cp_rel#/}"
    if [ -z "$_nros_cp_rel" ]; then
        printf '%s' "$_nros_cp_here_real"
    else
        printf '%s/%s' "$_nros_cp_here_real" "$_nros_cp_rel"
    fi
    unset _nros_cp_value _nros_cp_here _nros_cp_owner \
        _nros_cp_here_real _nros_cp_owner_real _nros_cp_rel
    return 0
}

# `build-root.sh` ships `nros_build_root` to its `make` leaves with `export -f`,
# and a leaf gets the function but never sources this file — so the two helpers
# it now calls have to travel with it. bash-only; every other shell sources
# this file directly.
if [ -n "${BASH_VERSION:-}" ]; then
    export -f nros_checkout_root nros_reroot_checkout_path 2>/dev/null || true
    export NROS_CHECKOUT_MARKER
fi
