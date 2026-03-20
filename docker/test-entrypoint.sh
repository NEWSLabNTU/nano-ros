#!/bin/bash
# Docker entrypoint for privileged integration tests.
#
# Phase 1 (root): network setup
# Phase 2 (host user): run tests
#
# File capabilities (setcap) don't work on Docker bind-mounted volumes,
# so the builder user gets passwordless sudo for the test command.
# This lets ThreadX Linux binaries open AF_PACKET raw sockets.
#
# Required env vars:
#   HOST_UID, HOST_GID — host user identity (for file ownership)
#
# Required capabilities:
#   CAP_NET_ADMIN — create veth pairs, bridges, set IPs
#   CAP_NET_RAW   — ThreadX Linux AF_PACKET raw sockets
set -e

HOST_UID="${HOST_UID:-1000}"
HOST_GID="${HOST_GID:-1000}"

# ============================================================
# Phase 1: Root — network setup + user creation
# ============================================================

echo "=== [root] Setting up test environment ==="

# Create the host user account (idempotent)
groupadd -g "$HOST_GID" -o builder 2>/dev/null || true
useradd -u "$HOST_UID" -g "$HOST_GID" -o -m -d /home/builder -s /bin/bash builder 2>/dev/null || true

# Fix cargo/rustup ownership for the host user
mkdir -p /cargo/registry /cargo/git /rustup/tmp /rustup/downloads
chown -R "$HOST_UID:$HOST_GID" /cargo/registry /cargo/git 2>/dev/null || true
chown "$HOST_UID:$HOST_GID" /cargo /rustup 2>/dev/null || true

# Allow builder passwordless sudo for running tests that need raw sockets.
# File capabilities (setcap) don't work on Docker bind-mounted volumes,
# so we grant full sudo instead of per-binary capabilities.
echo "builder ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/builder 2>/dev/null || true

# Run network setup scripts (creates bridges, veth pairs, TAP devices)
if [ -f /work/scripts/qemu/setup-network.sh ]; then
    bash /work/scripts/qemu/setup-network.sh builder 2>&1 | grep -v '^$' || true
fi

echo "=== [root] Setup complete ==="

# ============================================================
# Phase 2: Drop to host user — run tests
# ============================================================

if [ $# -eq 0 ]; then
    echo "No command specified. Usage: docker-test <just-recipe>"
    exit 1
fi

echo "=== [uid=$HOST_UID] Running: $* ==="

# Use capsh to drop to builder with ambient CAP_NET_RAW + CAP_NET_ADMIN.
# Ambient capabilities are inherited by all child processes, so ThreadX
# Linux binaries can open AF_PACKET raw sockets without setcap.
# (File capabilities via setcap don't work on Docker bind-mounted volumes.)
exec capsh \
    --user=builder \
    --inh=cap_net_raw,cap_net_admin \
    --addamb=cap_net_raw,cap_net_admin \
    -- -c "exec $(printf '%q ' "$@")"
