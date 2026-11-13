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
nros_nuttx_kconfig_frontend || exit 1

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

# Configure the way a user does: NuttX's own configure over the board config
# (a fragment), then nano-ros and the app switched on with `kconfig-tweak`, as
# the NuttX book page says, and resolved again.
nros_nuttx_configure "$work/nuttx" "$APPDIR" "$defconfig" "$work/nuttx/$makedefs" \
    "$work/configure" >/dev/null || exit 1
(
    cd "$work/nuttx"
    kconfig-tweak --file .config --enable CONFIG_NROS --enable CONFIG_NROS_MAKE_ROAD_SMOKE
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

# --- 4. an incremental build reaches the archive (issue 2495) ---------------
# A second plain `make` after a Rust input changed must rebuild the archive:
# NuttX runs an app's `context` once per stamp, so steps run only there gave
# a first build and never another. The input touched is the generated image
# crate's own source, under this run's build dir, so no checkout file moves.
archive="$(find "$work/apps/external/.nros-build" \( -path '*/release/libnros_c.a' -o -path '*/release/libnros_cpp.a' \) | head -n 1)" || archive=""
image_src="$(find "$work/apps/external/.nros-build" -path '*/image/nros-c*/src/lib.rs' | head -n 1)" || image_src=""
if [ -z "$archive" ] || [ -z "$image_src" ]; then
    echo "make-road-build [$id]: no archive or image crate source under $work/apps/external/.nros-build" >&2
    exit 1
fi
before="$(stat -c %y "$archive")"
sleep 1
touch "$image_src"
rc=0
(
    cd "$work/nuttx"
    python3 "$ROOT/scripts/build/make-stall-watchdog.py" \
        --label "nuttx make road ($id, incremental)" --diag-dir "$work" -- \
        make -j"$(nproc)"
) >"$work/make-incremental.log" 2>&1 || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "make-road-build [$id]: the incremental make FAILED (exit $rc); see $work/make-incremental.log" >&2
    tail -n 15 "$work/make-incremental.log" >&2
    exit 1
fi
after="$(stat -c %y "$archive")"
if [ "$after" = "$before" ]; then
    echo "make-road-build [$id]: an edited Rust input did not rebuild $archive on the next make" >&2
    exit 1
fi
echo "make-road-build [$id]: guest OK — $elf ($(stat -c %s "$elf") bytes, $nros_count nros_* functions); an edit rebuilt the archive"

# --- 5. a PROJECT build (phase-550 W6c, issue 2236 section 5) ---------------
# An app names a nano-ros project (`NROS_PROJECTS` in its Make.defs), and the
# same make then configures that project's image (W6a), hands cargo its facts
# (W6b) and sizes it from the probe `nros configure` ran (W6d). The project is
# an in-tree example whose image is for this kernel's board, copied into this
# run's apps tree as a user's would be.
#
# Only where an in-tree example's image is for the config's board: the arm
# config builds `qemu-armv7a-nuttx`, and no example has an `rv-virt-nuttx`
# image yet, so the riscv cell builds the guest case alone.
case "$id" in
    arm) proj_src="$ROOT/examples/qemu-armv7a-nuttx/c/talker" ;;
    *) proj_src="" ;;
esac
if [ -z "$proj_src" ]; then
    echo "make-road-build [$id]: no in-tree example has an image for this board; project case not run"
    exit 0
fi

# run_make <label> <log>: one plain `make` in the kernel tree, under the
# stall watchdog; fails the lane naming the log.
run_make() {
    local label="$1" mlog="$2" mrc=0
    (
        cd "$work/nuttx"
        python3 "$ROOT/scripts/build/make-stall-watchdog.py" \
            --label "nuttx make road ($id, $label)" --diag-dir "$work" -- \
            make -j"$(nproc)"
    ) >"$mlog" 2>&1 || mrc=$?
    if [ "$mrc" -ne 0 ]; then
        echo "make-road-build [$id]: the $label make FAILED (exit $mrc); see $mlog" >&2
        grep -nE '(^|[^a-z_])(error|Error)([:[ ]|$)|\*\*\*' "$mlog" | cut -c1-300 | head -n 20 >&2 || true
        tail -n 15 "$mlog" >&2
        exit 1
    fi
}

proj="$APPDIR/external/nros-make-road-project"
mkdir -p "$proj"
cp -r "$proj_src/src" "$proj/src"
bringup_toml="$(find "$proj/src" -path '*_bringup/system.toml' | head -n 1)" || bringup_toml=""
if [ -z "$bringup_toml" ]; then
    echo "make-road-build [$id]: $proj_src has no bringup system.toml" >&2
    exit 1
fi
# A make app's components state their entities (W6a, issue 2236 Q6); these
# are the talker's own.
sed -i 's|^name = "talker"$|&\nentities = ["publisher:std_msgs/msg/String:/chatter", "timer"]|' "$bringup_toml"
nros_grep_q -F 'entities = [' "$bringup_toml" || {
    echo "make-road-build [$id]: could not declare the talker's entities in $bringup_toml" >&2
    exit 1
}
# shellcheck disable=SC2016  # $(APPDIR) is make's variable
echo 'NROS_PROJECTS += $(APPDIR)/external/nros-make-road-project' \
    >> "$APPDIR/external/nros-make-road-smoke/Make.defs"

plog="$work/make-project.log"
echo "make-road-build [$id]: project make (log: $plog)"
run_make project "$plog"

make_dir="$(find "$work/apps/external/.nros-build" -path '*/image/make/image-key' -printf '%h\n' | head -n 1)" || make_dir=""
facts="$make_dir/nros-image-facts.toml"
if [ -z "$make_dir" ] || [ ! -f "$facts" ]; then
    echo "make-road-build [$id]: the project build wrote no image facts under $work/apps/external/.nros-build" >&2
    exit 1
fi
key="$(cat "$make_dir/image-key")"
# The image is configured, and current.
located=""
located="$("$NROS_CLI" locate configured --project "$proj" --image "$key")" || located=""
if ! nros_grep_q -Fx $'current\tyes' <<<"$located"; then
    echo "make-road-build [$id]: \`$key\` is not configured (or not current) after the project make" >&2
    exit 1
fi
# It is sized: a descriptor named to cargo, written from the probe.
descriptor="$(sed -n '/^\[env\.NROS_SIZING_DESCRIPTOR\]/,/^\[/ s/^value = "\(.*\)"$/\1/p' "$facts")"
if [ -z "$descriptor" ] || [ ! -f "$descriptor" ]; then
    echo "make-road-build [$id]: $facts names no sizing descriptor" >&2
    exit 1
fi
if ! nros_grep_q -Fx 'basis = "probe"' "$descriptor"; then
    echo "make-road-build [$id]: $descriptor is not the probe's (basis: $(grep '^basis' "$descriptor"))" >&2
    exit 1
fi
if nros_grep_q -F 'built without a sizing descriptor' "$plog"; then
    echo "make-road-build [$id]: the project build warned it is unsized; see $plog" >&2
    exit 1
fi

# A second make with nothing changed: configure is a no-op and the archive
# is not rebuilt.
stamp="$(sed -n 's/^configured\t//p' <<<"$located")/stamp.toml"
stamp_before="$(stat -c %y "$stamp")"
archive_before="$(stat -c %y "$archive")"
sleep 1
run_make "project, unchanged" "$work/make-project-again.log"
if [ "$(stat -c %y "$stamp")" != "$stamp_before" ] || [ "$(stat -c %y "$archive")" != "$archive_before" ]; then
    echo "make-road-build [$id]: a second make with nothing changed reconfigured or rebuilt the archive" >&2
    exit 1
fi

# Editing a knob row in the app's system.toml reaches the archive on the next
# make, with no clean.
printf '\n[image.qemu-armv7a-nuttx.knobs.zenoh.wire]\nbatch_unicast_size = 2048\n' >> "$bringup_toml"
sleep 1
run_make "project, knob edited" "$work/make-project-knob.log"
if ! nros_grep_q -F 'NROS_ZENOH_BATCH_UNICAST_SIZE = "2048"' "$make_dir/nros-image-env.toml"; then
    echo "make-road-build [$id]: the edited knob row is not in $make_dir/nros-image-env.toml" >&2
    exit 1
fi
if [ "$(stat -c %y "$archive")" = "$archive_before" ]; then
    echo "make-road-build [$id]: an edited knob row did not rebuild $archive on the next make" >&2
    exit 1
fi
echo "make-road-build [$id]: project OK — \`$key\` configured in the make, sized from the probe ($descriptor); a second make was a no-op; a knob edit rebuilt the archive"
