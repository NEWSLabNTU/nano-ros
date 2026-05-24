#!/usr/bin/env fish
# nano-ros activation file (fish).
#
# Source this file once per shell session to put every shipped
# nano-ros binary on PATH:
#
#   source ./setup.fish
#   zenohd --listen tcp/127.0.0.1:7447 &
#   nros --help
#
# Idempotent — re-sourcing rebuilds PATH entries without duplicating.
# Skips dirs that don't exist (e.g. before `just setup`); re-source
# after the build to pick them up.

set -l _nros_script (status --current-filename)
set -gx NROS_ROOT (realpath (dirname $_nros_script))

# Export repo-local SDK defaults from the just/sdk-env.just SSoT.
# Existing caller-provided variables are preserved by scripts/sdk-env.sh.
if test -f "$NROS_ROOT/scripts/sdk-env.sh"; and type -q bash; and type -q just
    eval (bash "$NROS_ROOT/scripts/sdk-env.sh" --fish)
end

# Binary directories shipped by nano-ros builds.
set -l _nros_bin_dirs \
    "$HOME/.local/bin" \
    "$NROS_ROOT/build/zenohd" \
    "$NROS_ROOT/build/qemu/bin" \
    "$NROS_ROOT/build/xrce-agent" \
    "$NROS_ROOT/packages/codegen/packages/target/release"

# Strip any previous nano-ros entries from PATH before re-adding.
set -l _nros_clean
for p in $PATH
    if test "$p" != "$HOME/.local/bin"
        and not string match -q "$NROS_ROOT/build/*" -- $p
        and not string match -q "$NROS_ROOT/packages/codegen/*" -- $p
        set -a _nros_clean $p
    end
end
set -gx PATH $_nros_clean

# Prepend each existing dir (last-listed first so the first entry
# ends up frontmost).
set -l _nros_added
for d in $_nros_bin_dirs[-1..1]
    if test -d "$d"
        set -gx PATH $d $PATH
        set -a _nros_added $d
    end
end

# Reverse _nros_added so the banner prints in the canonical order.
set -l _nros_added_ordered
for i in (seq (count $_nros_added) -1 1)
    set -a _nros_added_ordered $_nros_added[$i]
end

# Convenience env vars pointing at canonical binaries.
function _nros_set_if_exists
    set -l var $argv[1]
    set -l path $argv[2]
    if test -x "$path"
        set -gx $var $path
    end
end

_nros_set_if_exists NROS_ZENOHD          "$NROS_ROOT/build/zenohd/zenohd"
_nros_set_if_exists NROS_QEMU_SYSTEM_ARM "$NROS_ROOT/build/qemu/bin/qemu-system-arm"
_nros_set_if_exists NROS_XRCE_AGENT      "$NROS_ROOT/build/xrce-agent/MicroXRCEAgent"
_nros_set_if_exists NROS_CODEGEN         "$NROS_ROOT/packages/codegen/packages/target/release/nros-codegen"
_nros_set_if_exists NROS_CLI             "$NROS_ROOT/packages/codegen/packages/target/release/nros"

functions -e _nros_set_if_exists

# Confirmation banner.
if status --is-interactive
    and not test "$NROS_QUIET_SETUP" = "1"
    echo "[nano-ros] NROS_ROOT=$NROS_ROOT"
    if test (count $_nros_added_ordered) -eq 0
        echo "[nano-ros] No shipped binaries on PATH yet — run 'just setup' first."
    else
        echo "[nano-ros] Binaries on PATH:"
        for d in $_nros_added_ordered
            echo "[nano-ros]   $d"
        end
    end
end
