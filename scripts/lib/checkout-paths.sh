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
# still attributable) and may name a FILE rather than a directory — the walk is
# `${d%/*}` on the string, so nothing here asks the filesystem what kind of
# thing it was handed.
#
# That last property has no live example as of phase-468 W2: every variable
# `sdk-env.just` now exports is directory-valued, and the one that named a file,
# `NROS_ESP_IDF_ENV_SHIM`, went with the ESP-IDF port. It is stated anyway
# because it is a property of the CONTRACT, not of the current caller set — a
# future file-valued root must not have to rediscover that this is allowed.
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

# nros_reroot_checkout_pathlist <list> <here>
#
# The same rule, applied to each entry of a `:`-separated search list — PATH.
# Issue 1638: the rule above reached NAMED variables, and a tool resolved by
# NAME through PATH crossed checkouts anyway. A worktree whose parent shell
# put `<main>/third-party/ninja` first on PATH configured its Zephyr leaf with
# `CMAKE_MAKE_PROGRAM=<main>/third-party/ninja/ninja`, and
# `check-zephyr-workspace-foreign-checkout` (correctly) refused the cache.
#
# Per entry:
#   outside any checkout, or inside THIS one   -> kept, unchanged
#   inside ANOTHER checkout, counterpart here  -> re-rooted here
#   inside ANOTHER checkout, no counterpart    -> DROPPED
#
# The third row is what a list adds over a single value. A single path is
# re-rooted even if absent (its consumer reports the missing directory by
# name), but a PATH entry that does not exist is skipped SILENTLY by every
# lookup, so re-rooting `<main>/third-party/ninja` to a worktree that has no
# such directory would be a no-op and leave nothing to report — and keeping
# the original is exactly the crossing. Dropping it lets the lookup fall
# through to the next entry, which is the documented fallback (the SDK
# store's tool, else the system's).
#
# Order and empty entries are preserved. The owning-checkout walk is done
# inline (no subshell) so a 40-entry PATH costs a fork only for the entries a
# checkout actually owns — `just` evaluates this on every invocation.
nros_reroot_checkout_pathlist() {
    _nros_pl_rest="${1:-}"
    _nros_pl_here="${2:-}"
    _nros_pl_out=""
    _nros_pl_first=1
    while :; do
        case "$_nros_pl_rest" in
            *:*)
                _nros_pl_entry="${_nros_pl_rest%%:*}"
                _nros_pl_rest="${_nros_pl_rest#*:}"
                _nros_pl_more=1
                ;;
            *)
                _nros_pl_entry="$_nros_pl_rest"
                _nros_pl_more=0
                ;;
        esac
        _nros_pl_keep=1
        _nros_pl_new="$_nros_pl_entry"
        _nros_pl_d="$_nros_pl_entry"
        _nros_pl_owned=0
        case "$_nros_pl_d" in
            /*)
                while [ -n "$_nros_pl_d" ] && [ "$_nros_pl_d" != "/" ]; do
                    if [ -f "$_nros_pl_d/$NROS_CHECKOUT_MARKER" ]; then
                        _nros_pl_owned=1
                        break
                    fi
                    _nros_pl_d="${_nros_pl_d%/*}"
                done
                ;;
        esac
        if [ "$_nros_pl_owned" = 1 ] && [ -n "$_nros_pl_here" ]; then
            _nros_pl_new="$(nros_reroot_checkout_path "$_nros_pl_entry" "$_nros_pl_here")"
            if [ "$_nros_pl_new" != "$_nros_pl_entry" ] && [ ! -d "$_nros_pl_new" ]; then
                _nros_pl_keep=0
            fi
        fi
        if [ "$_nros_pl_keep" = 1 ]; then
            if [ "$_nros_pl_first" = 1 ]; then
                _nros_pl_out="$_nros_pl_new"
                _nros_pl_first=0
            else
                _nros_pl_out="$_nros_pl_out:$_nros_pl_new"
            fi
        fi
        [ "$_nros_pl_more" = 1 ] || break
    done
    printf '%s' "$_nros_pl_out"
    unset _nros_pl_rest _nros_pl_here _nros_pl_out _nros_pl_first _nros_pl_entry \
        _nros_pl_more _nros_pl_keep _nros_pl_new _nros_pl_d _nros_pl_owned
    return 0
}

# nros_checkout_scope_name <here>
#
# Issues 1904/1921 — the name a checkout's own subtree carries under a build
# root it shares with other checkouts: `<basename>-<sha256(realpath)[:12]>`.
# The basename is for a human reading the directory listing; the hash is what
# separates two checkouts that share a basename. Hashed over the RESOLVED path
# (`pwd -P`), so a checkout reached through a symlinked parent is one tree, not
# two. Prints nothing and fails when `<here>` is not a directory.
#
# Mirrored by `nros_build_paths::checkout_scope_name` (Rust) and
# `_nros_checkout_scope_name` in `cmake/NanoRosCheckoutRoot.cmake`. The cross
# pins: `nros_build_paths`'s `checkout_scope_tests` RUN this function against
# the Rust one, and `check-cargo-dir-knob-key.sh` compares the cmake answer to
# `nros_checkout_scoped_root` below.
nros_checkout_scope_name() {
    _nros_cs_real="$(cd "${1:-}" 2>/dev/null && pwd -P)" || _nros_cs_real=""
    if [ -z "$_nros_cs_real" ]; then
        unset _nros_cs_real
        return 1
    fi
    if command -v sha256sum >/dev/null 2>&1; then
        _nros_cs_hash="$(printf '%s' "$_nros_cs_real" | sha256sum)"
    else
        _nros_cs_hash="$(printf '%s' "$_nros_cs_real" | shasum -a 256)"
    fi
    printf '%s-%.12s' "${_nros_cs_real##*/}" "${_nros_cs_hash%% *}"
    unset _nros_cs_real _nros_cs_hash
    return 0
}

# nros_checkout_scoped_root <root> <here>
#
# Issue 1921 — the ONE checkout-ownership rule for a BUILD ROOT. A build root
# holds one checkout's build cache (RFC-0070 R1). `NROS_BUILD_ROOT` lets an
# operator move it to a faster volume, and a linked worktree inherits its parent
# shell's value, so without this every checkout that sets it shares every kind
# under it: fixture trees and stamps, the sizes probe, the lane-skip ledgers.
# Sharing there buys nothing — each checkout reaches its crates by its own path,
# so a cargo dir holds two units of every crate (issue 0616) — and a ledger or
# stamp then answers for the wrong tree.
#
#   <root> owned by <here>    -> <root>, unchanged
#   anything else             -> <root>/checkouts/<nros_checkout_scope_name>
#
# "Owned" means either:
#   - the DEEPEST checkout above <root> is <here>, compared after resolving
#     symlinks (the issue-1280/1391 rule, never a lexical prefix: a worktree
#     nested under `<main>/.claude/worktrees/` is not part of `<main>`). This is
#     the default `<checkout>/build`, so no warm cache moves; or
#   - <root> already lies inside <here>'s own `checkouts/<name>` scope. That is
#     what makes the rule IDEMPOTENT: a consumer handed an already-scoped path
#     (the corrosion-cargo kind dir, passed to cmake as
#     `NROS_SHARED_CARGO_ROOT`) asks the same question and gets the same path
#     back, never `…/checkouts/x/…/checkouts/x`.
#
# Unlike `nros_reroot_checkout_path`, a root inside ANOTHER checkout is scoped,
# not re-rooted: `NROS_BUILD_ROOT` is never inherited from an activated shell
# (nothing exports it), so naming a sibling's tree is an operator choice and is
# honoured — inside this checkout's own subtree of it.
# Always prints a path; a `<here>` that cannot be resolved keeps `<root>`.
nros_checkout_scoped_root() {
    _nros_sr_root="${1:-}"
    _nros_sr_here="${2:-}"
    if [ -z "$_nros_sr_root" ] || [ -z "$_nros_sr_here" ]; then
        printf '%s' "$_nros_sr_root"
        unset _nros_sr_root _nros_sr_here
        return 0
    fi
    _nros_sr_here_real="$(cd "$_nros_sr_here" 2>/dev/null && pwd -P)" || _nros_sr_here_real=""
    _nros_sr_name="$(nros_checkout_scope_name "$_nros_sr_here")" || _nros_sr_name=""
    if [ -z "$_nros_sr_here_real" ] || [ -z "$_nros_sr_name" ]; then
        printf '%s' "$_nros_sr_root"
        unset _nros_sr_root _nros_sr_here _nros_sr_here_real _nros_sr_name
        return 0
    fi
    _nros_sr_owned=0
    _nros_sr_owner="$(nros_checkout_root "$_nros_sr_root")"
    if [ -n "$_nros_sr_owner" ]; then
        _nros_sr_owner_real="$(cd "$_nros_sr_owner" 2>/dev/null && pwd -P)" || _nros_sr_owner_real=""
        [ "$_nros_sr_owner_real" = "$_nros_sr_here_real" ] && _nros_sr_owned=1
    fi
    case "$_nros_sr_root/" in
        */checkouts/"$_nros_sr_name"/*) _nros_sr_owned=1 ;;
    esac
    if [ "$_nros_sr_owned" = 1 ]; then
        printf '%s' "$_nros_sr_root"
    else
        printf '%s/checkouts/%s' "$_nros_sr_root" "$_nros_sr_name"
    fi
    unset _nros_sr_root _nros_sr_here _nros_sr_here_real _nros_sr_name \
        _nros_sr_owned _nros_sr_owner _nros_sr_owner_real
    return 0
}

# `build-root.sh` ships `nros_build_root` to its `make` leaves with `export -f`,
# and a leaf gets the function but never sources this file — so the helpers
# it calls have to travel with it. bash-only; every other shell sources
# this file directly.
if [ -n "${BASH_VERSION:-}" ]; then
    export -f nros_checkout_root nros_reroot_checkout_path \
        nros_checkout_scope_name nros_checkout_scoped_root 2>/dev/null || true
    export NROS_CHECKOUT_MARKER
fi
