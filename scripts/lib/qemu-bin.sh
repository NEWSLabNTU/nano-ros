# shellcheck shell=bash
# qemu-bin.sh — which QEMU system binary to run (issue 1851).
#
# One ladder, the shell twin of `nros_qemu_ladder::locate` (the Rust crate the
# CLI and `nros_tests::qemu::qemu_system_path` both call, phase-517 W14):
#
#   1. `QEMU_SYSTEM_<ARCH>` (e.g. `QEMU_SYSTEM_ARM`) — an explicit override.
#   2. The project-local prefix `build/qemu/bin` ($NROS_KIND_QEMU; made by
#      `just qemu setup-qemu` / `just linux setup`).
#   3. The `nros setup` store at the PINNED version:
#      `<store>/sdk/<tool>/<pin>/bin` — constructed from the pin, never by
#      listing the store (issue 1546).
#   4. `$PATH`.
#
# Before this, each recipe searched its own subset: `just freertos`/`just nuttx`
# only rung 2, the RISC-V and ESP32 recipes only rung 4, so a host whose QEMU
# was in the store (what `nros setup` produces) passed the QEMU tests while the
# recipes found nothing. `check-qemu-one-ladder` keeps a bare `qemu-system-*`
# launch out of `just/` and `scripts/`.
#
# Sourceable; defines functions only. Reads no CLI, so it works before `nros`
# is built.

_NROS_QEMU_BIN_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# shellcheck source=../build/build-root.sh
. "$_NROS_QEMU_BIN_LIB_DIR/../build/build-root.sh"
# shellcheck source=store-root.sh
. "$_NROS_QEMU_BIN_LIB_DIR/store-root.sh"
# shellcheck source=sdk-pin.sh
. "$_NROS_QEMU_BIN_LIB_DIR/sdk-pin.sh"

# _nros_qemu_prefix_current <prefix> <tool> <binary>
#
# Issue 0930, the shell twin of `nros_tests::qemu::project_prefix_matches_pin`:
# succeeds when `<prefix>/.nros-provenance` (written by `nros setup --prefix`)
# names the version `[tool.<tool>]` pins, else explains on stderr and fails. No
# readable pin means nothing to compare against: the prefix is taken as chosen.
_nros_qemu_prefix_current() {
    local prefix="$1" tool="$2" bin="$3" pin held
    pin="$(nros_sdk_pinned_version "$tool" 2>/dev/null)" || return 0
    if [ ! -f "$prefix/.nros-provenance" ]; then
        held="no .nros-provenance (not installed by nros setup), so an unknown version"
    else
        held="$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\(.*\)"[[:space:]]*$/\1/p' "$prefix/.nros-provenance" | head -1)"
        [ "$held" = "$pin" ] && return 0
        held="${held:-a marker naming no version}"
    fi
    printf '%s\n' \
        "nano-ros: $bin is not the QEMU the index pins ($pin): it holds $held." \
        "  A run must not use an emulator nobody chose (issue 0930). Re-provision" \
        "  it with \`just qemu setup-qemu\`, or remove $prefix to use the store copy." >&2
    return 1
}

# nros_qemu_prefix_check
#
# Issue 0930 — the precondition form of rung 2's rule, for
# `check-tier-preconditions`: succeeds when there is no project prefix (rung 2
# simply does not answer) or when it holds the pinned version; otherwise prints
# why and fails.
nros_qemu_prefix_check() {
    local prefix bin
    prefix="$(nros_build_dir "$NROS_KIND_QEMU")"
    bin="$prefix/bin/qemu-system-arm"
    [ -x "$bin" ] || return 0
    _nros_qemu_prefix_current "$prefix" qemu "$bin"
}

# nros_qemu_bin <exe> [<tool>] [noprefix]
#
# Prints the binary to run for `<exe>` (`qemu-system-arm`, …). `<tool>` is the
# index row whose store copy is rung 3 (default `qemu`); `noprefix` skips rung
# 2, for a QEMU the project prefix never holds (the ESP32 fork,
# `[tool.esp32-qemu]`). When no rung answers it prints the bare name and
# returns 1, so a caller can either run it (and get the shell's own "not
# found") or test the status. A stale project prefix returns 2 with nothing on
# stdout and the reason on stderr (issue 0930).
nros_qemu_bin() {
    local exe="${1:?nros_qemu_bin: binary name}" tool="${2:-qemu}" mode="${3:-}"
    local arch="${exe#qemu-system-}" var cand ver root
    var="QEMU_SYSTEM_$(printf '%s' "$arch" | tr '[:lower:]-' '[:upper:]_')"
    if [ -n "${!var:-}" ]; then
        printf '%s\n' "${!var}"
        return 0
    fi
    if [ "$mode" != noprefix ]; then
        cand="$(nros_build_dir "$NROS_KIND_QEMU" bin)/$exe"
        if [ -x "$cand" ]; then
            # Issue 0930 — the prefix is one directory across pin bumps, so it
            # answers only while it holds the pinned version; a stale or undated
            # one is refused LOUDLY (nothing on stdout, status 2), never skipped
            # for a lower rung nobody chose either.
            if ! _nros_qemu_prefix_current "$(nros_build_dir "$NROS_KIND_QEMU")" "$tool" "$cand"; then
                return 2
            fi
            printf '%s\n' "$cand"
            return 0
        fi
    fi
    if ver="$(nros_sdk_pinned_version "$tool")" && root="$(nros_store_root 2>/dev/null)"; then
        cand="$root/sdk/$tool/$ver/bin/$exe"
        if [ -x "$cand" ]; then
            printf '%s\n' "$cand"
            return 0
        fi
    fi
    if cand="$(command -v "$exe" 2>/dev/null)"; then
        printf '%s\n' "$cand"
        return 0
    fi
    printf '%s\n' "$exe"
    return 1
}

# nros_qemu_report <exe> <min-version|-> [<tool>] [noprefix]
#
# One doctor line for `<exe>`, naming the binary the ladder picks: `[OK]`,
# or `[MISSING]` (status 1) when no rung answers or the binary is older than
# `<min-version>` (`-` for no floor). The ARM floor is 7.2: `-netdev
# dgram,local.type=unix` arrived there, and the NuttX/ThreadX multi-instance
# cells need it (phase-127 B.5). Every doctor reports through this, so a
# doctor cannot pass on a binary the recipes would not run.
nros_qemu_report() {
    local exe="${1:?nros_qemu_report: binary name}" min="${2:--}" tool="${3:-qemu}"
    local bin ver major minor fmaj fmin
    if ! bin="$(nros_qemu_bin "$exe" "$tool" "${4:-}")"; then
        echo "  [MISSING] $exe (run: nros setup --tool $tool — prebuilt, no sudo)"
        return 1
    fi
    ver="$("$bin" --version 2>/dev/null | head -1 | sed -E 's/^[^0-9]*([0-9]+\.[0-9]+).*/\1/')"
    if [ "$min" != - ]; then
        major="${ver%%.*}" minor="${ver##*.}" fmaj="${min%%.*}" fmin="${min##*.}"
        if [ -z "$ver" ] || [ "$major" -lt "$fmaj" ] \
            || { [ "$major" -eq "$fmaj" ] && [ "$minor" -lt "$fmin" ]; }; then
            echo "  [MISSING] $exe >= $min (the ladder found ${ver:-an unreadable version} at $bin;"
            echo "            run: nros setup --tool $tool — prebuilt, no sudo)"
            return 1
        fi
    fi
    echo "  [OK] $exe${ver:+ $ver}: $bin"
}
