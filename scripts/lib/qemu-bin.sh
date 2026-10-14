# shellcheck shell=bash
# qemu-bin.sh — which QEMU system binary to run (issue 1851).
#
# One ladder, the shell twin of `nros_tests::qemu::qemu_system_path`:
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

# nros_qemu_bin <exe> [<tool>] [noprefix]
#
# Prints the binary to run for `<exe>` (`qemu-system-arm`, …). `<tool>` is the
# index row whose store copy is rung 3 (default `qemu`); `noprefix` skips rung
# 2, for a QEMU the project prefix never holds (the ESP32 fork,
# `[tool.esp32-qemu]`). When no rung answers it prints the bare name and
# returns 1, so a caller can either run it (and get the shell's own "not
# found") or test the status.
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
