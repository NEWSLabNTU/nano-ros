#!/usr/bin/env bash
# Deprecated: scripts/setup.sh has been replaced by `just setup`.
#
# Run one of:
#   just setup            # install workspace + all platforms + services
#   just doctor           # diagnose install status
#   just <module> setup   # install just one module (e.g. just nuttx setup)
#   just <module> doctor  # diagnose one module (e.g. just zephyr doctor)
#
# This shim forwards to `just setup` so legacy CI / docs still work.
# It will be removed in a future cleanup.

set -e
echo "WARNING: scripts/setup.sh is deprecated. Use 'just setup' instead." >&2
exec just setup "$@"
