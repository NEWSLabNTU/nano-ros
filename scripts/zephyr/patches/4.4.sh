#!/usr/bin/env bash
# Phase 199.3 — Zephyr 4.4-line patch set (the version-dispatched applier
# `scripts/zephyr/patches/<NROS_ZEPHYR_VERSION>.sh`). Lifted verbatim from the
# old inline `if NROS_ZEPHYR_VERSION = 4.4` branch in just/zephyr.just.
#
# Contract: one positional arg, the Zephyr workspace dir. Runs from the repo
# root regardless of cwd. Each patch is idempotent. Adding a new Zephyr line is
# a new sibling `<version>.sh` — no applier edit (see patches/README.md).
set -e

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$REPO_ROOT"
WORKSPACE="${1:?usage: patches/4.4.sh <zephyr-workspace-dir>}"

# CHECKS by default; CREATES only when the caller owns its interpreter and says
# so. A CI container is that caller — the zephyr CI image ships `uv` for exactly
# this ("uv (Python 3.12 for the Zephyr 4.4 line)") — and nothing ever ran it, so
# every 4.4 cell died here from the day provisioning was removed. Threaded as an
# env var rather than a separate workflow step so the venv PATH stays derived in
# one place instead of pasted into three jobs.
CREATE_VENV=""
[ "${NROS_ZEPHYR_CREATE_VENV:-0}" = "1" ] && CREATE_VENV="--create"
echo "[zephyr setup] 4.4 line: Python 3.12 venv (Zephyr 4.4 requires >=3.12)${CREATE_VENV:+ — creating}..."
bash ./scripts/zephyr/provision-py312-venv.sh "$WORKSPACE" $CREATE_VENV
echo "[zephyr setup] 4.4 line: applying 4.4 NSOS patches (Phase 180.A)..."
# Task 5 — NSOS recvmsg (cyclonedds UDP receive). Task 6 — NSOS
# IPv4-multicast forwarding (SPDP discovery): guest half first
# (constants + struct + nsos_sockets.c marshalling), then the host
# adapt forwarder. Both re-anchored to the 4.4 nsos shape.
bash ./scripts/zephyr/nsos-recvmsg-patch-4.4.sh "$WORKSPACE"
bash ./scripts/zephyr/native-sim-ipproto-ip-patch-4.4.sh "$WORKSPACE"
bash ./scripts/zephyr/nsos-adapt-ipproto-ip-patch-4.4.sh "$WORKSPACE"
# Cyclone DDS runtime: relax pthread_mutex_unlock to Linux/glibc
# NORMAL semantics (Zephyr 4.x k_mutex owner-only-unlock aborts
# ddsrt's xevent thread right after the first publish).
bash ./scripts/zephyr/pthread-mutex-unlock-patch-4.4.sh "$WORKSPACE"
# phase-474 I16: Cortex-M ITCM/DTCM output sections SUBALIGN(4) -> ALIGN(4)
# (backport of upstream de3deb75d44d + c456d2b1279b, not in v4.4.0). Without it
# eDMA's __aligned(32) TCD pool in DTCM can land 8-aligned and the first DMA
# transfer asserts in EDMA_TcdSetTransferConfig.
bash ./scripts/zephyr/tcm-section-align-patch-4.4.sh "$WORKSPACE"
# Issue 2187 — the zephyr-lang-rust gpio pair (issue 0432) is NOT a 3.7-only
# patch: 4.4's pin (a763400) still emits a one-cell `GpioPin::new` call for
# mps2/an385's `#gpio-cells = <1>` controller, so every Rust mps2 image failed
# to compile the `zephyr` crate on this line (4 x E0061). The same hunks apply.
echo "[zephyr setup] 4.4 line: applying zephyr-lang-rust gpio patch (idempotent)..."
bash ./scripts/zephyr/zephyr-lang-rust-gpio-patch.sh "$WORKSPACE"
# Issue 2203 — CMSIS_6 includes <arm_acle.h>, and with the C compiler's builtin
# include dir on bindgen's `-I` list clang parses GCC's copy and fails
# (`argument to '__builtin_arm_cdp' must be a constant integer`), so no Rust
# Cortex-M image builds on this line without it.
echo "[zephyr setup] 4.4 line: applying zephyr-lang-rust bindgen include patch (idempotent)..."
bash ./scripts/zephyr/zephyr-lang-rust-bindgen-include-patch.sh "$WORKSPACE"
echo "[zephyr setup] 4.4 line: legacy 3.7 Rust/cyclone-submodule patches skipped (Tasks 4-9)"
