---
id: 901
title: "The artifact-identity gate read a path nothing writes, and told you to
  run a build that does not produce it"
status: resolved
type: bug
area: build, testing
related: [issue-0499, issue-0616, phase-340]
resolved_in: "issue 0901"
---

## Symptom

Two failure modes, opposite in appearance, one cause.

On a long-lived machine the gate reported a budget breach — `nros` at 8
identities against a ceiling of 5. On a fresh checkout it SKIPPED, because the
tree it reads did not exist. Its own remedy, followed literally, fixed neither.

## Cause: a hardcoded path with no producer

    TREE="${NROS_IDENTITY_BUDGET_TREE:-examples/workspaces/mixed/build-workspace-fixtures}"

Nothing writes that directory any more. The workspace build gained PER-PLATFORM
suffixes — `build-workspace-fixtures-freertos`, `-threadx`, … — and the
unsuffixed name was left behind. Every `examples/workspaces/mixed` row in
`fixtures.toml` now builds into `build/posix-zenoh-native/cmake` and siblings.

So the directory survives only as residue on machines old enough to predate the
rename, and what the gate measured there was accumulation: rlibs cargo never
collects, from builds before the layout changed. 8 identities of history,
reported as a regression.

Deleting it — which the gate suggests — did not help either, because no lane
recreates it.

## And the advice was wrong

    BUILD_HINT="… just build-test-fixtures lane=native"

Measured: a full `lane=native` run (**2 616 s**) left every
`examples/workspaces/*/build-workspace-fixtures*` at its previous mtime. Those
trees come from the WORKSPACE build, not that lane. Advice that does not produce
the artifact is worse than none — it costs the reader forty minutes and leaves
the gate exactly as silent as before.

## Fix

* **Resolve the tree, do not name it.** Take the newest
  `examples/workspaces/*/build-workspace-fixtures*` that actually contains
  cargo output, so the gate reads what the last build produced whatever the
  layout is called this month. `NROS_IDENTITY_BUDGET_TREE` still overrides, and
  the historical name remains the fallback so the SKIP message points somewhere
  recognisable.
* **Name a command that builds it**: `just build-test-fixtures`.
* `just prune-artifacts` carried the SAME dead path as its default — the sibling
  of the class, fixed with it.

## Verified

* Before: `[SKIP] … no build tree at …/mixed/build-workspace-fixtures`.
* After: the gate finds a real tree (333 rlibs in `features/`) and reports
  honestly that they predate the current `started_at` — "this tree is history,
  not that build".
* Given a tree WITH in-window artifacts it measures:
  `nros_core 3/4 identities; worst crate 3/5; worst identity 1/5 copies`.
* `--self-test`: 2/2.

## The residual skip is correct, and worth stating

On an up-to-date tree a fixture build compiles nothing, so no artifact falls
inside the `started_at` window and the gate skips. That is issue 0499's design
working: it counts THIS build's artifacts, never history. The gate measures
after a build that actually compiled, and says plainly when it cannot — which
is the opposite of the silent green it used to give.
