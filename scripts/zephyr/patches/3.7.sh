#!/usr/bin/env bash
# Phase 199.3 — Zephyr 3.7-line (LTS) patch set (the version-dispatched applier
# `scripts/zephyr/patches/<NROS_ZEPHYR_VERSION>.sh`). Lifted verbatim from the
# old inline `else` (3.7) branch in just/zephyr.just.
#
# Contract: one positional arg, the Zephyr workspace dir. Runs from the repo
# root regardless of cwd. Each patch is idempotent. Adding a new Zephyr line is
# a new sibling `<version>.sh` — no applier edit (see patches/README.md).
#
# Most of these are native_sim / NSOS / CycloneDDS-on-native_sim shims that edit
# Zephyr internals (not stable APIs) — the churn Phase 199 tracks for upstreaming.
set -e

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$REPO_ROOT"
WORKSPACE="${1:?usage: patches/3.7.sh <zephyr-workspace-dir>}"

echo "[zephyr setup] applying Cortex-A9 / SLCR patches (idempotent)..."
bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$WORKSPACE"
echo "[zephyr setup] applying AArch64 / Cortex-R Rust patches (idempotent)..."
bash ./scripts/zephyr/aarch64-rust-patch.sh "$WORKSPACE"
bash ./scripts/zephyr/cortex-r-rust-patch.sh "$WORKSPACE"
echo "[zephyr setup] applying Phase 168.1 cargo-features patch (idempotent)..."
bash ./scripts/zephyr/cargo-features-patch.sh "$WORKSPACE"
echo "[zephyr setup] applying Phase 97.4 native_sim IPPROTO_IP patch (idempotent)..."
bash ./scripts/zephyr/native-sim-ipproto-ip-patch.sh "$WORKSPACE"
echo "[zephyr setup] applying Phase 168.X.fvp llext-edk conditional patch (idempotent)..."
bash ./scripts/zephyr/llext-edk-conditional-patch.sh "$WORKSPACE"
echo "[zephyr setup] applying Phase 11W.6 cyclonedds threads patch (idempotent)..."
bash ./scripts/zephyr/cyclonedds-zephyr-threads-patch.sh
echo "[zephyr setup] applying Phase 11W.7 cyclonedds log eager-flush patch (idempotent)..."
bash ./scripts/zephyr/cyclonedds-zephyr-log-flush-patch.sh
echo "[zephyr setup] applying Phase 11W.7/.8 cyclonedds udp sockopt patch (idempotent)..."
bash ./scripts/zephyr/cyclonedds-zephyr-udp-rcvbuf-patch.sh
# NSOS (Native Simulator Offloaded Sockets) patches edit the consumer's
# Zephyr tree — they MUST get "$WORKSPACE". Called bare, they fall back to
# nano-ros's in-tree `../nano-ros-workspace` default, which does not exist for
# a downstream consumer (a board-provisioning run via `nros setup board`).
# These edit native_sim driver files (nsos_*.c/h) that exist in stock Zephyr
# regardless of board, so they apply idempotently and are inert for non-
# native_sim targets (e.g. FVP, which does not compile the NSOS driver).
echo "[zephyr setup] applying Phase 11W.8 NSOS getsockname patch (idempotent)..."
bash ./scripts/zephyr/nsos-getsockname-patch.sh "$WORKSPACE"
echo "[zephyr setup] applying Phase 11W.10 NSOS recvmsg patch (idempotent)..."
bash ./scripts/zephyr/nsos-recvmsg-patch.sh "$WORKSPACE"
echo "[zephyr setup] applying Phase 11W.11 NSOS mcast-join dual-mreq patch (idempotent)..."
bash ./scripts/zephyr/nsos-mcjoin-mreq-patch.sh "$WORKSPACE"
echo "[zephyr setup] applying Phase 11W.12 NSOS getifaddrs patch (idempotent)..."
bash ./scripts/zephyr/nsos-getifaddrs-patch.sh "$WORKSPACE"
echo "[zephyr setup] applying Phase 11W.12 NSOS adapt IPPROTO_IP setsockopt patch (idempotent)..."
bash ./scripts/zephyr/nsos-adapt-ipproto-ip-patch.sh "$WORKSPACE"
echo "[zephyr setup] applying Phase 11W.8 cyclonedds multicast-join best-effort patch (idempotent)..."
bash ./scripts/zephyr/cyclonedds-zephyr-mcjoin-patch.sh
echo "[zephyr setup] applying Phase 11W.8 cyclonedds sockwaitset self-pipe patch (idempotent)..."
bash ./scripts/zephyr/cyclonedds-zephyr-sockwaitset-patch.sh
