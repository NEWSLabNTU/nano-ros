#!/usr/bin/env bash
#
# What the Zephyr patch sets (`scripts/zephyr/patches/<line>.sh`) require of the
# workspace they are handed, and what they do with the path.
#
# Two defects, both reported from `nros setup board <b> --zephyr-workspace .`:
#
#   * The three Rust-target patches and the cargo-features patch exited 1 when
#     `modules/lang/rust` was absent, so a C-only workspace (no zephyr-lang-rust
#     in west.yml, which the book's zephyr-west-app chapter calls legitimate)
#     could not be set up at all. The module is OPTIONAL; `zephyr/` is not.
#   * `patches/<line>.sh` ran `cd "$REPO_ROOT"` before reading its argument, so
#     a RELATIVE workspace resolved against the SDK root, not the caller's cwd.
#
# Every case asserts what the scripts PRINTED as well as their status: a skip
# that says nothing is as wrong as a failure, and "exited 0" alone would pass a
# script that silently did nothing.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
z="$root/scripts/zephyr"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fail() {
    echo "[FAIL] $*" >&2
    exit 1
}

# A run whose status we inspect, never letting `set -e` take the test down at
# the assignment (issue 1249).
run() {
    rc=0
    out="$("$@" 2>&1)" || rc=$?
}

rust_only=(aarch64-rust-patch cortex-r-rust-patch cargo-features-patch
    zephyr-lang-rust-gpio-patch zephyr-lang-rust-bindgen-include-patch)

# 1. C-only workspace: `zephyr/` present, no `modules/lang/rust`. Every
#    Rust-module patch exits 0 AND says it skipped.
c_ws="$tmp/c-only"
mkdir -p "$c_ws/zephyr"
for s in "${rust_only[@]}" cortex-a9-rust-patch; do
    run bash "$z/$s.sh" "$c_ws"
    [ "$rc" -eq 0 ] || fail "$s on a C-only workspace exited $rc:
$out"
    grep -qi "no zephyr-lang-rust module\|no rust module" <<<"$out" ||
        fail "$s skipped the absent module without saying so:
$out"
done

# 2. No `zephyr/` at all: still a hard failure, for every one of them.
bare="$tmp/not-a-workspace"
mkdir -p "$bare/modules/lang/rust"
for s in aarch64-rust-patch cortex-r-rust-patch cargo-features-patch cortex-a9-rust-patch; do
    run bash "$z/$s.sh" "$bare"
    [ "$rc" -ne 0 ] || fail "$s accepted a directory with no zephyr/:
$out"
    grep -q "missing zephyr/" <<<"$out" || fail "$s failed without naming zephyr/:
$out"
done

# 3. The module present: the Rust-target patches APPLY, and a second run is a
#    no-op. Minimal files carrying exactly the anchors the patches key on.
r_ws="$tmp/with-rust"
mkdir -p "$r_ws/zephyr" "$r_ws/modules/lang/rust"
printf 'function(_rust_map_target)\n  if(CONFIG_CPU_CORTEX_M)\n  elseif(CONFIG_RISCV)\n  endif()\nendfunction()\n' \
    >"$r_ws/modules/lang/rust/CMakeLists.txt"
printf 'config RUST_SUPPORTED\n\tbool\n\tdefault y if ((CPU_CORTEX_M || RISCV) && !TOOLCHAIN_ARCMWDT)\n' \
    >"$r_ws/modules/lang/rust/Kconfig"
for s in aarch64-rust-patch cortex-r-rust-patch cortex-a9-rust-patch; do
    run bash "$z/$s.sh" "$r_ws"
    [ "$rc" -eq 0 ] || fail "$s on a workspace WITH the module exited $rc:
$out"
    grep -q '^\[apply\]' <<<"$out" || fail "$s applied nothing to an unpatched module:
$out"
    run bash "$z/$s.sh" "$r_ws"
    [ "$rc" -eq 0 ] || fail "$s second run exited $rc:
$out"
    if grep -q '^\[apply\]' <<<"$out"; then
        fail "$s is not idempotent:
$out"
    fi
done
grep -q 'aarch64-unknown-none' "$r_ws/modules/lang/rust/CMakeLists.txt" ||
    fail "aarch64-rust-patch left no AArch64 case in CMakeLists.txt"

# 4. A RELATIVE workspace, from a cwd that is not the SDK root. Both line sets
#    must hand their scripts the path the CALLER meant. The C-only workspace
#    cannot complete either set (the native_sim patches need real Zephyr
#    sources), so the assertion is on the PATH the scripts report, and on the
#    one message the bug produced never appearing.
caller="$tmp/caller"
mkdir -p "$caller/ws/zephyr"
for line in 3.7 4.4; do
    rc=0
    out="$(cd "$caller" && bash "$z/patches/$line.sh" ws 2>&1)" || rc=$?
    if grep -q "doesn't look like a Zephyr workspace" <<<"$out"; then
        fail "patches/$line.sh resolved a relative workspace against the SDK root:
$out"
    fi
    grep -qF "$caller/ws/" <<<"$out" ||
        fail "patches/$line.sh never named the caller's workspace $caller/ws:
$out"
    if grep -qF "$root/ws/" <<<"$out"; then
        fail "patches/$line.sh named a workspace under the SDK root:
$out"
    fi
done

echo "zephyr-patch-set-workspace: OK (C-only skips said so, zephyr/ still required, apply + idempotent, relative path kept)"
