#!/usr/bin/env bash
#
# Phase 230.0.2 (RFC-0034) — no-direct-kernel-allocator gate.
#
# RFC-0034 makes `nros_platform_alloc` / `_dealloc` / `_realloc` the sole
# allocation funnel: only a platform PORT may call the host kernel's
# allocator directly (that is how it implements the ABI). Every other nros
# crate — RMW shims, the language wrappers' `#[global_allocator]`, core —
# must route through `nros_platform_alloc`, so a single counter sees the
# true C+Rust heap total (closes issue #6).
#
# This gate scans nros's OWN source (the vendored zenoh-pico submodule and
# mbedtls are out of scope — Wave 1 guards the vendored scalar defs behind
# a fork `#ifdef` separately) for direct references to kernel allocator
# symbols. A hit is allowed only in:
#   * a platform port crate (packages/core/nros-platform-*, packages/platforms/*),
#   * the KNOWN_BYPASS allowlist below (sites Phase 230 Wave 1 migrates).
# A hit ANYWHERE ELSE is a NEW bypass and fails the build immediately.
#
# Wave 1.7 empties KNOWN_BYPASS (after nros-c/nros-cpp route through the
# ABI) and this gate becomes fully hard.
#
# Hooked from `just check`.

set -euo pipefail

# Kernel allocator symbols that must only appear inside a platform port.
# (Allocation only — thread/sync/net primitives are a later wave.)
# Word-boundaried so the short names (`k_malloc`/`k_free`) don't substring-
# match `task_free` / `zsock_freeaddrinfo` etc.
SYMBOLS='\b(pvPortMalloc|vPortFree|tx_byte_allocate|tx_byte_release|heap_caps_malloc|heap_caps_free|k_malloc|k_free)\b'

# Roots to scan (nros-owned C + Rust).
ROOTS='packages'

# Paths that are allowed to reference the symbols (regex, matched against
# the repo-relative path).
ALLOW_RE='(packages/core/nros-platform-[^/]+/|packages/platforms/[^/]+/)'

# Vendored opaque-struct task/net carve-outs (RFC-0034). ThreadX exposes a
# SINGLE allocation primitive (`tx_byte_allocate`/`tx_byte_release`) used for
# BOTH the scalar heap AND task stacks / NetX packet pools. The scalar heap is
# already funneled through nros-platform-threadx's `nros_platform_alloc`; the
# remaining board-crate sites carve out *thread stacks* and *NetX net pools* —
# the TASK and NET opaque-struct services, which RFC-0034 keeps per-RTOS-
# vendored (NOT part of the scalar-alloc unification). Allowed only for the
# ThreadX byte-pool symbols, only in these board files — a stray scalar
# pvPortMalloc/k_malloc here would still fail.
TASK_NET_ALLOW_RE='(packages/boards/nros-board-common/c/threadx_hooks\.c|packages/boards/nros-board-threadx-qemu-riscv64/c/board_threadx_qemu_riscv64\.c)'
TASK_NET_SYM_RE='\b(tx_byte_allocate|tx_byte_release)\b'

# Out-of-scope trees: vendored submodules + build output. Not nros source.
EXCLUDE_RE='(zpico-sys/zenoh-pico/|zpico-sys/mbedtls/|/target/|/out/|\.lock$|\.ld$)'

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Collect every hit (path:line:text), drop excluded trees and comments.
mapfile -t hits < <(
    grep -rInE "$SYMBOLS" $ROOTS 2>/dev/null \
        | grep -vE "$EXCLUDE_RE" \
        | grep -vE ':[[:space:]]*(//|\*|#)' \
        || true
)

bypass=()
for line in "${hits[@]}"; do
    path="${line%%:*}"
    [[ "$path" =~ $ALLOW_RE ]] && continue  # legitimate port
    # Vendored task/net carve-out: ThreadX byte-pool symbol in a board file.
    if [[ "$path" =~ $TASK_NET_ALLOW_RE && "$line" =~ $TASK_NET_SYM_RE ]]; then
        continue
    fi
    bypass+=("$line")
done

# HARD mode (Phase 230.1.7, RFC-0034). The nros-OWNED allocation surface is
# fully migrated: nros-c/nros-cpp `#[global_allocator]`s + the C-API inline
# platform headers + the FreeRTOS/orin board task-context sites all route
# through `nros_platform_alloc`; the ThreadX board task-stack / NetX net-pool
# carve-outs are the vendored task/net opaque-struct services (see
# TASK_NET_ALLOW_RE). A NEW direct kernel-alloc in nros source now fails the
# build. The vendored zenoh-pico scalar funnel (Wave 1c) is enforced
# SEPARATELY by the fork `#ifndef Z_FEATURE_NROS_PLATFORM_ALLOC` guard +
# its CI relink lane — it is out of THIS gate's scope (EXCLUDE_RE drops the
# submodule), so it is not a precondition for hard mode here.
# Escape hatch: `NROS_ALLOC_GATE_HARD=0` reverts to advisory for triage.
HARD_FAIL="${NROS_ALLOC_GATE_HARD:-1}"

if ((${#bypass[@]} == 0)); then
    echo "✓ no-direct-kernel-alloc: clean (all allocation routes through nros_platform_alloc)"
    exit 0
fi

echo "ℹ no-direct-kernel-alloc (advisory): ${#bypass[@]} direct kernel-allocator reference(s) outside a platform port"
echo "  — Phase 230 Wave 1 worklist (RFC-0034). Route each through nros_platform_alloc/_dealloc/_realloc."
printf '   %s\n' "${bypass[@]}"

if [[ "$HARD_FAIL" == "1" ]]; then
    echo "✗ no-direct-kernel-alloc: hard-fail mode (NROS_ALLOC_GATE_HARD=1) — bypass sites remain." >&2
    exit 1
fi
exit 0
