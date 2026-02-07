#!/bin/bash
# Default Docker entrypoint that drops privileges to the host user.
#
# If HOST_UID/HOST_GID are set and non-zero, creates a matching user and
# runs the command as that user. Otherwise runs as root (backwards compatible).
#
# This entrypoint is overridden by docker-compose services that need their
# own entrypoint (e.g., the network-aware entrypoint.sh for QEMU tests).

HOST_UID="${HOST_UID:-0}"
HOST_GID="${HOST_GID:-0}"

if [ "$HOST_UID" != "0" ]; then
    groupadd -g "$HOST_GID" -o builder 2>/dev/null || true
    useradd -u "$HOST_UID" -g "$HOST_GID" -o -m -d /home/builder -s /bin/bash builder 2>/dev/null || true
    mkdir -p /cargo/registry /cargo/git /rustup/tmp /rustup/downloads /rustup/update-hashes
    chown -R "$HOST_UID:$HOST_GID" /cargo/registry /cargo/git 2>/dev/null || true
    chown -R "$HOST_UID:$HOST_GID" /rustup/tmp /rustup/downloads /rustup/update-hashes 2>/dev/null || true
    chown "$HOST_UID:$HOST_GID" /cargo /rustup 2>/dev/null || true
    exec gosu builder "$@"
fi

exec "$@"
