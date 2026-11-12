#!/usr/bin/env bash
# phase-550 W1 — build the NuttX make road (the `apps/external` guest road,
# `integrations/nuttx/`) the way a NuttX user does, for one board config.
#
#   make-road-build.sh <config-id>      # arm | riscv (a dir under nuttx-config/)
#
# What it does, in a tree that belongs to this run alone:
#
#   1. Clones its OWN copy of the pinned kernel and apps trees under
#      build/nuttx-make-road/<id>/ — never the shared `third-party/nuttx/nuttx`
#      (issue 0525: one `.config` per tree, and a lane must not move it).
#   2. Configures the kernel with the board's defconfig, stages the shell
#      (`stage-external-apps.sh`), sets `CONFIG_NROS=y`, and adds a minimal C
#      app (`tests/nuttx-make-road-app/`) that calls the API, so the link
#      pulls the nano-ros archive in rather than linking nothing.
#   3. Runs `make`, and checks the kernel ELF holds the app's entry, the API
#      function it calls, and `nros_*` text from the archive.
#
# Exit 0 only when all three hold. Nothing here restates a NuttX fact: the
# board Make.defs path is derived from the defconfig's own CONFIG_ARCH /
# CONFIG_ARCH_CHIP / CONFIG_ARCH_BOARD, and the cross tools are whatever that
# Make.defs picks.
#
# The trees are cloned from wherever the source ladder finds them
# (`nros locate`, RFC-0103 D3) at that tree's HEAD, so an uncommitted edit in a
# local kernel checkout is NOT built here; commit it, or point NUTTX_DIR at a
# tree whose HEAD carries it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

id="${1:?usage: make-road-build.sh <config-id>  (arm | riscv)}"
defconfig="$ROOT/packages/boards/nros-board-nuttx-qemu/nuttx-config/$id/defconfig"
if [ ! -f "$defconfig" ]; then
    echo "make-road-build: no board config '$id' (looked for $defconfig)" >&2
    exit 2
fi

# shellcheck source=scripts/build/cargo.sh
source "$ROOT/scripts/build/cargo.sh"
# shellcheck source=scripts/lib/grep-q.sh
source "$ROOT/scripts/lib/grep-q.sh"
# shellcheck source=scripts/nuttx/kconfig-frontend.sh
source "$ROOT/scripts/nuttx/kconfig-frontend.sh"

if ! NROS_CLI="$(nros_cli_bin)"; then
    echo "make-road-build: no nros CLI — run 'just setup-cli' (the shell's Make.defs asks it where the archive lands)" >&2
    exit 1
fi
export NROS_CLI

# --- 1. this run's own trees ------------------------------------------------
work="$ROOT/build/nuttx-make-road/$id"
rm -rf "$work"
mkdir -p "$work"

# own_tree <source-name> <dest>
own_tree() {
    local name="$1" dest="$2" src head
    if ! src="$(nros_locate_source "$name")" || [ -z "$src" ]; then
        echo "make-road-build: '$name' is provisioned nowhere this build looks — run 'nros setup --source $name'" >&2
        return 1
    fi
    if head="$(git -C "$src" rev-parse --verify -q HEAD 2>/dev/null)"; then
        # `--shared` borrows the objects, so a clone costs a checkout, not a copy
        # of the history. The tree is this run's alone either way.
        git clone --quiet --shared --no-checkout "$src" "$dest"
        git -C "$dest" checkout --quiet --detach "$head"
        echo "  $name: $src @ ${head:0:12}"
    else
        cp -a "$src" "$dest"
        echo "  $name: $src (copied; not a git tree)"
    fi
}

echo "make-road-build [$id]: trees under $work"
own_tree nuttx-kernel "$work/nuttx"
own_tree nuttx-apps "$work/apps"

# --- 2. configure -----------------------------------------------------------
nros_nuttx_kconfig_frontend "$ROOT"

defval() { sed -n "s/^$1=\"\{0,1\}\([^\"]*\)\"\{0,1\}$/\1/p" "$defconfig" | head -1; }
arch="$(defval CONFIG_ARCH)"
chip="$(defval CONFIG_ARCH_CHIP)"
board="$(defval CONFIG_ARCH_BOARD)"
makedefs="boards/$arch/$chip/$board/scripts/Make.defs"
if [ -z "$arch" ] || [ -z "$chip" ] || [ -z "$board" ] || [ ! -f "$work/nuttx/$makedefs" ]; then
    echo "make-road-build: $defconfig does not name a board in this kernel (derived '$makedefs')" >&2
    exit 1
fi

export APPDIR="$work/apps"
bash "$ROOT/scripts/nuttx/stage-external-apps.sh" "$APPDIR" >/dev/null

# The smoke app, staged the way a user stages theirs: a directory under
# apps/external, and one `source` line in the generated apps/external/Kconfig
# (NuttX's kconfig has no `osource` glob, see stage-external-apps.sh).
# COPIED, not linked: Application.mk writes objects beside the sources, which
# through a link would land in the nano-ros checkout (the shell avoids that
# with its own PREFIX; a user's app lives in their own tree, as this copy does).
cp -r "$ROOT/tests/nuttx-make-road-app" "$APPDIR/external/nros-make-road-smoke"
# shellcheck disable=SC2016  # $APPSDIR is kconfig's variable, expanded by kconfig
echo 'source "$APPSDIR/external/nros-make-road-smoke/Kconfig"' >> "$APPDIR/external/Kconfig"

(
    cd "$work/nuttx"
    ln -sf "$work/nuttx/$makedefs" Make.defs
    cp "$defconfig" .config
    {
        # What `tools/configure.sh -a <appdir>` writes: the kernel finds its
        # apps tree through this symbol (tools/Unix.mk), not through $APPDIR.
        echo "CONFIG_APPS_DIR=\"$APPDIR\""
        echo "CONFIG_NROS=y"
        echo "CONFIG_NROS_MAKE_ROAD_SMOKE=y"
    } >> .config
    make olddefconfig >/dev/null
)
for sym in CONFIG_NROS CONFIG_NROS_MAKE_ROAD_SMOKE; do
    if ! nros_grep_q -Fx "$sym=y" "$work/nuttx/.config"; then
        echo "make-road-build: $sym=y did not survive olddefconfig (a dependency is unmet in $id's defconfig)" >&2
        exit 1
    fi
done

# --- 3. build ---------------------------------------------------------------
log="$work/make.log"
echo "make-road-build [$id]: make (log: $log)"
rc=0
(
    cd "$work/nuttx"
    # issue 1403 — every `make -j` runs under the stall watchdog.
    python3 "$ROOT/scripts/build/make-stall-watchdog.py" \
        --label "nuttx make road ($id)" --diag-dir "$work" -- \
        make -j"$(nproc)"
) >"$log" 2>&1 || rc=$?
if [ "$rc" -ne 0 ]; then
    # NuttX's make interleaves download and patch chatter with the error, so
    # the tail alone can hide it: print the error lines first, then the tail.
    echo "make-road-build [$id]: FAILED (make exit $rc). Error lines of $log:" >&2
    grep -nE '(^|[^a-z_])(error|Error)([:[ ]|$)|\*\*\*' "$log" | cut -c1-300 | head -n 30 >&2 || true
    echo "--- last lines:" >&2
    tail -n 15 "$log" >&2
    exit 1
fi

elf="$work/nuttx/nuttx"
if [ ! -f "$elf" ]; then
    echo "make-road-build [$id]: make succeeded but wrote no $elf" >&2
    exit 1
fi
# `readelf` reads any ELF whatever the host's binutils target; `nm` does not.
funcs="$(readelf -sW "$elf" | awk '$4 == "FUNC" && $7 != "UND" { print $8 }')"
missing=""
for want in nros_make_road_smoke_main rcl_publish; do
    nros_grep_q -Fx "$want" <<<"$funcs" || missing="$missing $want"
done
nros_count="$(awk '/^nros_/ { n++ } END { print n + 0 }' <<<"$funcs")"
[ "$nros_count" -gt 0 ] || missing="$missing nros_*"
if [ -n "$missing" ]; then
    echo "make-road-build [$id]: $elf links, but lacks:$missing — the archive did not reach the image" >&2
    exit 1
fi
echo "make-road-build [$id]: OK — $elf ($(stat -c %s "$elf") bytes, $nros_count nros_* functions)"
