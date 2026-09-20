#!/usr/bin/env sh
# One value, put through the ONE re-rooting rule — issue 1391.
#
# `just/sdk-env.just` cannot apply `nros_reroot_checkout_path` in-process (it
# is `just`, not a shell), and it must not restate the rule: a LEXICAL prefix
# rewrite is not the rule, and the difference is measurable the moment two
# checkouts NEST. An agent worktree lives at `<main>/.claude/worktrees/<id>`,
# so the main checkout's root is a strict PREFIX of the worktree's, the
# rewrite fired on values that were already correct, and
# `NROS_PLATFORM_CFFI_INCLUDE` came out naming
# `<main>/.claude/worktrees/<id>/.claude/worktrees/<id>/packages/...`. Builds
# then died on `fatal error: nros/platform.h: No such file or directory`,
# naming a source file rather than the environment (issue 1391).
#
# The rule resolves the value's OWNING checkout — the DEEPEST marker at or
# above it — which is what makes nesting decidable: a worktree-rooted value is
# owned by the worktree, so it is kept, while a value rooted at the parent is
# owned by the parent, so it is re-rooted. See `checkout-paths.sh` for the
# three-valued rule itself.
#
# usage: reroot-checkout-path.sh <value> <here>
#
# Always exits 0 and always prints something: `just` aborts the whole run on a
# failed `shell()`, and there is no value here worth aborting a build over.

set -eu

# shellcheck source=scripts/lib/checkout-paths.sh
. "$(dirname "$0")/checkout-paths.sh"

nros_reroot_checkout_path "${1:-}" "${2:-}"
