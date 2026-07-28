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
# `nros ws sync` regenerates portable models (content-addressed staleness in
# `cmd/ws.rs` re-hashes recorded inputs and treats an absolute path as stale).
# This gate keeps a machine-specific path from ever being committed again.
set -euo pipefail
cd "$(dirname "$0")/.."

# Absolute `path:` values (list item `- path: /…` or nested `path: /…`). The
# structure/execution layers key on `file:`/`scope:`/`to:`, never `path:`, so
# this targets provenance only and will not trip on a ROS topic like
# `to: /remapped_out`.
abs_paths=$(grep -rnE '^[[:space:]]*(- )?(record[[:space:]]*)?path:[[:space:]]*/' \
  examples --include='*_model.yaml' 2>/dev/null || true)

# Belt-and-suspenders: a machine home directory anywhere in a model is always a
# non-portable leak, whatever key it hangs off.
home_dirs=$(grep -rnE '/home/|/Users/|/root/' \
  examples --include='*_model.yaml' 2>/dev/null || true)

if [ -n "${abs_paths}${home_dirs}" ]; then
  echo "check-no-absolute-model-paths: committed SystemModel(s) carry absolute host paths (issue 0320):" >&2
  printf '%s\n' "${abs_paths}" "${home_dirs}" | sed '/^$/d' >&2
  echo >&2
  echo "Fix: re-run \`nros ws sync\` in the affected workspace to regenerate portable models." >&2
  exit 1
fi

echo "check-no-absolute-model-paths: OK (all committed SystemModels portable)"
