#!/usr/bin/env sh
# A `:`-separated search list (PATH), put through the ONE re-rooting rule —
# issue 1638.
#
# The sibling of `reroot-checkout-path.sh`, for the same reason it exists:
# `just` cannot run the rule in-process and must not restate it. The justfile's
# `export PATH := …` calls this, so every recipe — and every tool a recipe
# resolves BY NAME (`ninja`, `make`, `cmake`, `nros`) — sees a PATH whose
# entries inside ANOTHER nano-ros checkout are re-rooted here, or dropped when
# this checkout has no such directory. See `nros_reroot_checkout_pathlist` in
# `checkout-paths.sh` for the three rows.
#
# usage: reroot-checkout-pathlist.sh <list> <here>
#
# Always exits 0 and always prints something: `just` aborts the whole run on a
# failed `shell()`, and an unchanged PATH is never worth aborting a build over.

set -eu

# shellcheck source=scripts/lib/checkout-paths.sh
. "$(dirname "$0")/checkout-paths.sh"

nros_reroot_checkout_pathlist "${1:-}" "${2:-}"
