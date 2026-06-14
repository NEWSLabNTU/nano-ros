#!/usr/bin/env bash
# Issue 0050 / phase-247 W1 — image-level weak-symbol checker.
#
# The source gate (`nros-tests/tests/weak_symbol_audit.rs`) proves no NEW
# unaudited weak *site* slips into the tree. This gate proves the other half:
# that each audited **override-default** weak symbol is actually
# **strong-overridden in the final linked image** — the real failure mode
# (a board forgets the strong def / `--gc-sections` drops it ⇒ the weak no-op
# silently wins, with no link error, only a runtime mis-behaviour).
#
# Method (validated on real artifacts, see phase-247 W1):
#   - Operate on FINAL linked images (firmware ELFs / executables), NEVER `.a`
#     archives — an input archive legitimately holds the weak default as `W`;
#     the override lands at the final link.
#   - `nm` each image. Per override-default symbol that an image is EXPECTED to
#     link strongly (the coverage map below):
#       * strong (`nm` type T/t/D/d/R/B/b) → OK (the override won);
#       * weak   (W/V/w/v)                 → FAIL (override dropped);
#       * absent                            → WARN (gc'd / not linked here —
#                                             informational, not a failure).
#   - One cross-arch tool: `llvm-nm` (reads thumbv7m ELFs identically to
#     `arm-none-eabi-nm`). Override with `NM=<nm>`.
#
# Coverage grows as artifact classes that link an override-default become
# prebuilt fixtures. Each row: a `find` base + name-glob, then the symbols.
# Missing artifacts are skipped (build-stage-fixture pattern — no compilation
# here; run after `just build-test-fixtures` / the per-platform fixture build).

set -uo pipefail

NM="${NM:-llvm-nm}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"

if ! command -v "$NM" >/dev/null 2>&1; then
    echo "weak-image: \`$NM\` not found — set NM=<nm>. Skipping." >&2
    exit 0
fi

# --- coverage map: "<find-base>|<name-glob>|<sym> <sym> …" -------------------
# Add a row when a new artifact class that links an override-default ships as a
# prebuilt fixture (cmake C/C++ images for the register dance, serial ELFs for
# the zenoh-pico aliases, threadx images, px4/uorb, …).
COVERAGE=(
    # FreeRTOS firmware: the board supplies strong netif hooks (LAN9118/lwIP).
    "examples/qemu-arm-freertos/rust|freertos_rs_*entry|nros_board_register_netif nros_board_poll_netif"
    "build/fixtures-cargo/qemu-arm-freertos|freertos_rs_*entry|nros_board_register_netif nros_board_poll_netif"
    # (phase-249 P4a removed the weak nros_app_register_backends default — it is now
    #  a generated strong def or a link error, never a weak-overridable symbol, so
    #  it left this image gate.)
    # Serial example ELFs (Phase 244.D1 Wave D): board serial aliases.
    "examples/qemu-arm-baremetal/rust|qemu-serial-talker|_z_open_serial_from_dev _z_close_serial _z_send_serial_internal _z_read_serial_internal"
    "examples/qemu-arm-baremetal/rust|qemu-serial-listener|_z_open_serial_from_dev _z_close_serial _z_send_serial_internal _z_read_serial_internal"
    # Bare-metal smoltcp net ELFs (MPS2-AN385 LAN9118): the `nros-smoltcp`
    # driver supplies strong `smoltcp_{init,cleanup}` (weak stubs in
    # zpico platform_aliases.c when smoltcp is absent).
    "examples/qemu-arm-baremetal/rust|qemu-bsp-talker|smoltcp_init smoltcp_cleanup"
    "examples/qemu-arm-baremetal/rust|qemu-bsp-listener|smoltcp_init smoltcp_cleanup"
    # ThreadX RISC-V64 firmware ELFs: the board overlay supplies a strong
    # `nros_board_init_eth` (NetX bring-up) and — phase-247 W3.2 — the app
    # stack/priority *getters*. A dropped override surfaces the symbol as weak
    # here: this row is the on-platform guard for the 155.A class (a 64 KB-default
    # stack overflow). (`_tx_initialize_low_level` is deliberately NOT here — it
    # is a weak SOLE def the board ships as overridable; see the allowlist.)
    "examples/qemu-riscv64-threadx/rust|qemu-riscv64-threadx-talker|nros_board_init_eth nros_board_app_stack_size nros_board_app_priority"
    "examples/qemu-riscv64-threadx/rust|qemu-riscv64-threadx-listener|nros_board_init_eth nros_board_app_stack_size nros_board_app_priority"
    # PENDING (no image target yet): px4 uorb `nros_orb_{register,unregister}_callback`
    # — strong in px4_callback_glue.cpp, but no Cargo.toml currently links
    # `nros-rmw-uorb` into an example/fixture, so there is no final image to nm.
    # Add a row here once a px4 uorb example ships (see phase-247 W1.2).
)

# A "strong" nm type letter: text/data/rodata/bss, upper (global) or lower (local).
is_strong() { case "$1" in T|t|D|d|R|r|B|b|A) return 0 ;; *) return 1 ;; esac; }
is_weak()   { case "$1" in W|V|w|v) return 0 ;; *) return 1 ;; esac; }

fails=0
warns=0
checked=0

check_artifact() {
    local artifact="$1"; shift
    local syms="$*"
    # `nm` lines: "<addr> <type> <name>" (undefined/weak-undef have no addr).
    local nmout
    nmout="$("$NM" "$artifact" 2>/dev/null)" || return 0
    local sym
    for sym in $syms; do
        # match the symbol as the last whitespace field, exactly.
        local line type
        line="$(printf '%s\n' "$nmout" | awk -v s="$sym" '$NF==s {print; exit}')"
        if [ -z "$line" ]; then
            echo "  WARN  $sym : absent in $artifact (gc'd / not linked here)"
            warns=$((warns + 1))
            continue
        fi
        # the type letter is the field before the name.
        type="$(printf '%s\n' "$line" | awk '{print $(NF-1)}')"
        checked=$((checked + 1))
        if is_weak "$type"; then
            echo "  FAIL  $sym : WEAK ($type) in $artifact — strong override DROPPED" >&2
            fails=$((fails + 1))
        elif is_strong "$type"; then
            echo "  ok    $sym : strong ($type) in $artifact"
        else
            echo "  WARN  $sym : type '$type' in $artifact (unexpected — review)"
            warns=$((warns + 1))
        fi
    done
}

# --- W1.3 SSoT cross-check: COVERAGE symbols vs the allowlist `[img:]` set ----
# The allowlist (scripts/weak-symbols-allowlist.txt) is the authority for WHICH
# override-default symbols are image-checkable; COVERAGE only maps them to the
# images that should link them strongly. Enforce: every COVERAGE symbol is
# declared `[img:]` there (a typo / un-audited symbol → FAIL), and report any
# declared symbol that has no coverage row yet (no silent gap). Runs even when
# no prebuilt images exist — it is a static consistency check.
allowlist="$repo_root/scripts/weak-symbols-allowlist.txt"
declare -A img_allow=()
if [ -f "$allowlist" ]; then
    while IFS= read -r toks; do
        for s in $toks; do img_allow["$s"]=1; done
    done < <(grep -E '^[0-9]' "$allowlist" | sed -n 's/.*\[img:\([^]]*\)\].*/\1/p')
fi
declare -A cov_syms=()
for row in "${COVERAGE[@]}"; do
    IFS='|' read -r _cb _cg csyms <<<"$row"
    for s in $csyms; do cov_syms["$s"]=1; done
done
ssot_fail=0
if [ "${#img_allow[@]}" -eq 0 ]; then
    echo "weak-image: WARN — no \`[img:]\` symbols parsed from the allowlist (SSoT check skipped)." >&2
else
    for s in "${!cov_syms[@]}"; do
        if [ -z "${img_allow[$s]:-}" ]; then
            echo "  FAIL  coverage symbol '$s' not declared [img:] in the allowlist (drift)" >&2
            ssot_fail=$((ssot_fail + 1))
        fi
    done
    for s in "${!img_allow[@]}"; do
        if [ -z "${cov_syms[$s]:-}" ]; then
            echo "  note  allowlisted override-default '$s' has no image-gate coverage row yet"
        fi
    done
fi

any_artifact=0
for row in "${COVERAGE[@]}"; do
    IFS='|' read -r base glob syms <<<"$row"
    [ -d "$base" ] || continue
    # Final images only: skip object/archive/intermediate artifacts.
    while IFS= read -r artifact; do
        case "$artifact" in *.a|*.o|*.rlib|*.rmeta|*.d) continue ;; esac
        any_artifact=1
        echo "== $artifact =="
        check_artifact "$artifact" $syms
    done < <(find "$base" -type f -name "$glob" 2>/dev/null \
                 ! -path '*/deps/*' ! -path '*/.fingerprint/*' ! -path '*/incremental/*')
done

echo
if [ "$ssot_fail" -gt 0 ]; then
    echo "weak-image: FAILED — coverage map drifted from the allowlist SSoT ($ssot_fail symbol(s), issue 0050 W1.3)." >&2
    exit 1
fi
if [ "$any_artifact" = 0 ]; then
    echo "weak-image: SSoT check passed; no covered prebuilt images found — run the fixture build first (image checks skipped)."
    exit 0
fi
echo "weak-image: checked=$checked fail=$fails warn=$warns"
if [ "$fails" -gt 0 ]; then
    echo "weak-image: FAILED — an override-default weak symbol was left weak in a final image (issue 0050)." >&2
    exit 1
fi
exit 0
