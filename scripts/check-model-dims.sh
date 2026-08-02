#!/usr/bin/env bash
#
# Issue 0380 — a committed SystemModel must not silently LOSE execution dims.
#
# WHAT IT PROTECTS
#
# `execution.tiers.*` carries hand-authored scheduling/placement dims —
# `class: real_time`, `zephyr.deadline_us`, `nuttx.budget_us`/`period_us`
# (sporadic), `threadx.preempt_threshold`/`time_slice_us`, per-platform `core`
# pins. `system.toml` deliberately does not carry them: the model YAML is the
# declared SSoT (phase-296 W5), because the resolver's inputs cannot express
# them.
#
# Which means anything that regenerates the model from inputs DELETES them.
# That happened twice (`07650d0a1`, `6071bd150`), stripping 17 dims, and
# nothing failed until ~17 realtime e2e tests reported the RFC-0052 fail-loud
# violation they exist to catch — a QEMU tier away from the cause.
#
# `nros sync` now refuses to shrink a model (the sync-time guard). This is the
# other half: it catches a model stripped by ANY means — a hand edit, an older
# CLI, a different tool — at `check-fast` speed rather than in a QEMU e2e run.
# Issue-0196 rule: the gate watches the same input the tests consume.
#
# ONE IMPLEMENTATION
#
# The dim set comes from `nros ws model-dims`, which is the same
# `execution_tier_dims()` the sync-time guard uses. Re-parsing YAML in shell
# would be a second spelling that could disagree about what a "dim" is — and it
# would get it wrong: `spin_period_us` is a tier dim while `nuttx.period_us` is
#
# The glob covers every `config/*model.yaml`, not just `system_model.yaml`: the
# variant models (talker/listener/multihost/...) carry no dims TODAY, but a
# gate scoped to the files that happen to have them now is narrower than the
# rule it enforces (issue 0196).
# the sporadic one, and a grep for `period_us` conflates them.

set -euo pipefail
cd "$(dirname "$0")/.."

BASELINE="scripts/model-dims-baseline.txt"

# shellcheck source=/dev/null
source scripts/build/cargo.sh

if ! nros_bin="$(nros_cli_bin 2>/dev/null)" || [ -z "$nros_bin" ]; then
    echo "model-dims: SKIP — no nros CLI available (run \`just setup-cli\`)."
    exit 0
fi

current="$(mktemp)"
baseline_sorted="$(mktemp)"
trap 'rm -f "$current" "$baseline_sorted"' EXIT

# A FAILING CLI IS NOT AN EMPTY MODEL (issue 0397).
#
# This loop used to read `… 2>/dev/null || true`, so any non-zero exit — most
# often the stale-CLI refusal after a rebase moves `packages/cli/` — produced
# zero dims for EVERY model and the diff below reported all 118 as LOST. That
# is the loudest failure this gate has, pointing at data loss that did not
# happen, and its own advice ("restore from git history rather than
# re-resolving") would have someone hand-editing generated files to fix a stale
# binary. The swallow also hid the one-line remedy the CLI already prints.
#
# So: a per-model failure is fatal and says which model and why. The `--write`
# path shares this loop, which matters more — re-recording from a broken CLI
# would BAKE the empty reading into the baseline and destroy the record the
# gate exists to keep.
# The marker is a FILE, not a variable: this loop is the left side of a
# pipeline, so it runs in a subshell and any variable it sets is gone by the
# time the parent reads it. (Written as a variable first; it silently never
# fired, which is the same shape as the bug being fixed.)
_dims_failed="$(mktemp)"
rm -f "$_dims_failed"
while IFS= read -r model; do
    if ! dims="$("$nros_bin" ws model-dims "$model" 2>&1)"; then
        printf 'model-dims: FAILED to read %s\n%s\n' "$model" "$dims" >&2
        : >"$_dims_failed"
        continue
    fi
    while IFS= read -r dim; do
        [ -z "$dim" ] && continue
        printf '%s\t%s\n' "$model" "$dim"
    done <<<"$dims"
done < <(git ls-files '*/config/*model.yaml') | sort -u >"$current"

if [ -e "$_dims_failed" ]; then
    rm -f "$_dims_failed"
    echo "" >&2
    echo "[FAIL] model-dims could not read every committed model — refusing to" >&2
    echo "       compare (or re-record) against a partial reading. Fix the CLI" >&2
    echo "       first; the error above says how." >&2
    exit 1
fi

# `--write` FIRST: it is the thing that creates the baseline, so gating it
# behind the baseline existing makes bootstrapping impossible. (The first draft
# did exactly that — the same shape as a repair tool blocked by the breakage it
# repairs.)
if [ "${1-}" = "--write" ]; then
    {
        echo "# Issue 0380 — execution.tiers dim keys each committed SystemModel"
        echo "# declares. Hand-authored dims the resolver CANNOT reproduce, so a"
        echo "# disappearing line is data loss, not a refresh."
        echo "#"
        echo "# Regenerate deliberately:  bash scripts/check-model-dims.sh --write"
        cat "$current"
    } >"$BASELINE"
    echo "wrote $BASELINE ($(grep -c . <"$current") dims)"
    exit 0
fi

if [ ! -f "$BASELINE" ]; then
    echo "[FAIL] $BASELINE is missing. Regenerate it with:" >&2
    echo "       bash scripts/check-model-dims.sh --write" >&2
    exit 1
fi

grep -vE '^\s*(#|$)' "$BASELINE" | sort -u >"$baseline_sorted"

lost="$(comm -23 "$baseline_sorted" "$current")"
added="$(comm -13 "$baseline_sorted" "$current")"

status=0
if [ -n "$lost" ]; then
    status=1
    echo "[FAIL] committed SystemModel(s) LOST execution dims (issue 0380):" >&2
    printf '%s\n' "$lost" | sed 's/^/       /' >&2
    echo "" >&2
    echo "       These are hand-authored and the resolver cannot put them back." >&2
    echo "       A regeneration almost certainly stripped them — restore from git" >&2
    echo "       history rather than re-resolving." >&2
fi
if [ -n "$added" ]; then
    status=1
    echo "[FAIL] committed SystemModel(s) gained execution dims not in the baseline:" >&2
    printf '%s\n' "$added" | sed 's/^/       /' >&2
    echo "" >&2
    echo "       Adding a dim is fine and deliberate — record it so the baseline" >&2
    echo "       keeps meaning something:" >&2
    echo "           bash scripts/check-model-dims.sh --write" >&2
fi
[ "$status" -eq 0 ] || exit 1

echo "model dims OK — $(grep -c . <"$current") dim(s) across $(cut -f1 <"$current" | sort -u | wc -l | tr -d ' ') model(s)."
