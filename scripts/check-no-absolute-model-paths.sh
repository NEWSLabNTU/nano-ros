#!/usr/bin/env bash
# Issue 0320 — committed SystemModels must be portable: no absolute host paths.
#
# `meta.inputs[].path` (and the legacy `meta.record.path`) are recorded RELATIVE
# to the bringup package so a committed `config/*_model.yaml` reproduces on any
# checkout. An absolute path bakes in the machine that last generated the model:
# it reproduces on exactly one host, and on every other one `main_macro`'s
# `system.toml` lookup silently falls through to `<bringup>/system.toml` — the
# per-target `[param_services]` leak the recording exists to prevent.
#
# `nros sync` regenerates portable models (content-addressed staleness in
# `cmd/ws.rs` re-hashes recorded inputs and treats an absolute path as stale).
# This gate keeps a machine-specific path from ever being committed again.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

# Absolute `path:` values (list item `- path: /…` or nested `path: /…`). The
# structure/execution layers key on `file:`/`scope:`/`to:`, never `path:`, so
# this targets provenance only and will not trip on a ROS topic like
# `to: /remapped_out`.
# `git grep`, not `grep -r`. Committed models are TRACKED, so the index already
# knows every one of them; `grep -r examples` instead walks each example's
# `target/` and `build/` output to rediscover files git could have named for
# free. Same defect as the `find` scans (scripts/check-no-tracked-file-find.sh):
# a pruned walk of `examples/` measured 7m36s against 0.8s for the index.
nros_git_grep abs_paths -nE '^[[:space:]]*(- )?(record[[:space:]]*)?path:[[:space:]]*/' \
  -- 'examples/**/*_model.yaml'

# Belt-and-suspenders: a machine home directory anywhere in a model is always a
# non-portable leak, whatever key it hangs off.
nros_git_grep home_dirs -nE '/home/|/Users/|/root/' \
  -- 'examples/**/*_model.yaml'

if [ -n "${abs_paths}${home_dirs}" ]; then
  echo "check-no-absolute-model-paths: committed SystemModel(s) carry absolute host paths (issue 0320):" >&2
  printf '%s\n' "${abs_paths}" "${home_dirs}" | sed '/^$/d' >&2
  echo >&2
  echo "Fix: re-run \`nros sync\` in the affected workspace to regenerate portable models." >&2
  exit 1
fi

echo "check-no-absolute-model-paths: OK (all committed SystemModels portable)"
