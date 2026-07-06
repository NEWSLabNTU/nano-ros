---
id: 147
title: "Plain-example test fixtures have no staleness detection — `require_prebuilt_binary` runs whatever binary is on disk, silently masking source drift"
status: open
type: tech-debt
area: testing
related: [132, 146, 129, 140]
---

## Summary

`build_example` → `require_prebuilt_binary`
(`nros-tests/src/fixtures/binaries/mod.rs`) is a bare existence check: if the
binary path exists, it's used — no comparison against the source it was built
from. So when an example's source changes but `just build-test-fixtures`
hasn't re-run, the test silently consumes the STALE binary and its result is
about old code. Workspace fixtures already guard this: they write a content
signature (`workspace-fixture-signature.sh` → `.nros-workspace-fixture.<id>.inputsig`)
and the resolver recomputes + fails "… is stale". Plain-example fixtures have
no equivalent.

This is the recurring hazard behind a whole class of confusing failures:
- **#146** — a stale `native/rust/listener` (pre-W4 `Int32_` vs the current
  `String_`) yielded 0-received "ros2→nano broken" before the real QoS defect
  was even reachable.
- **#129 / #140** — "stale June prebuilts masked months of lane rot" recurs in
  both root-cause writeups.
- The general pattern: a green local run proves nothing about current source
  unless fixtures were rebuilt, and nothing enforces that.

## Fix direction

Give plain-example fixtures the workspace-fixture treatment:
1. `fixtures-build.sh` (and the make-driver) writes a per-fixture inputsig —
   a content hash over the example dir's source files (mirror
   `workspace-fixture-signature.sh`'s scope: the example's own `*.rs/*.toml/
   *.c/*.h/CMakeLists.txt/package.xml`, target/ pruned).
2. `require_prebuilt_binary` (or a `build_example` wrapper) recomputes the
   signature and fails `… is stale: run just build-test-fixtures` on mismatch,
   exactly like the workspace path.

Scope note: like the workspace inputsig, this signs the example's OWN dir, not
its shared-crate deps — a pure `nros-core` edit wouldn't invalidate it. That's
an accepted limitation (fixtures are rebuilt wholesale in CI before the run);
the target is the far more common "edited the example, forgot to rebuild" and
"prebuilt is months old" cases.

## Detection today

A test whose subject clearly should deliver returns 0 / wrong type — check the
on-disk fixture's baked keyexpr/type (`strings <binary> | grep std_msgs`)
against current source before assuming a product bug.
