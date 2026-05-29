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

echo "[zephyr setup] 4.4 line: provisioning Python 3.12 venv (Zephyr 4.4 requires >=3.12)..."
bash ./scripts/zephyr/provision-py312-venv.sh "$WORKSPACE"
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
echo "[zephyr setup] 4.4 line: legacy 3.7 Rust/cyclone-submodule patches skipped (Tasks 4-9)"
