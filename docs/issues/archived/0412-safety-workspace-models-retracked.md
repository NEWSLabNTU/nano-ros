---
id: 412
title: "Eight SystemModel files are tracked again under examples/workspaces/safety — scooped into an unrelated fix, and check-fast is red"
status: resolved  # fixed 2026-08-04
type: bug
area: build
related: [rfc-0063, phase-330, phase-331, issue-0380]
---

## Symptom

`just check fast` fails on `main`:

```
check-no-tracked-models: tracked SystemModel files found:
  examples/workspaces/safety/src/demo_bringup/config/system_model.yaml
  … (8 total)
  Author the data in system.toml and re-run `nros sync`; never track the model.
```

The gate is [phase-330](../roadmap/phase-330-system-model-as-build-artifact.md)
W7.e, added precisely because [issue 0380](archived/0380-model-regeneration-destroys-hand-authored-execution-dims.md) was four hand-edit
deletions of committed models. It is working as designed; the invariant it
guards regressed.

## What happened

W4.a deleted all 112 tracked models on 2026-08-03. Eight came back in
`3f25803d1` ("fix(phase-331 W4 fallout): the zephyr rust safety entry resolved
the C catalog"), whose diffstat is **additions only**:

```
 …/config/c_safety_listener_model.yaml     | 32 ++++
 …/config/c_safety_talker_model.yaml       | 32 ++++
 …/config/cpp_safety_listener_model.yaml   | 32 ++++
 …/config/cpp_safety_talker_model.yaml     | 32 ++++
 …/config/rust_safety_listener_model.yaml  | 32 ++++
 …/config/rust_safety_talker_model.yaml    | 32 ++++
 …/config/rust_system_model.yaml           | 45 ++++++
 …/config/system_model.yaml                | 45 ++++++
 …/launch/rust_system.launch.xml           | 19 ++++
 …/demo_bringup/system.toml                |  6 ++
```

The **intended** change is the last two lines — a rust-only launch file plus its
`system.toml` entry. The eight model yamls are `nros sync` output that got
committed alongside it: the CLAUDE.md `git add -A` class, where a blanket add
scoops generated artifacts into an unrelated fix.

## Diagnosis — the fix IS safe, contrary to first appearance

A first look suggested the models no longer round-trip, which would have made
deletion risky. Measured, they do:

**Seven of the eight regenerate with identical content.** Running `nros sync` in
`examples/workspaces/safety` and diffing tracked against
`build/nros/models/demo_bringup/` gives, for `system_model.yaml`:

```diff
7c7
<     sha256: 6d9bfe0a67171339123ce996a06e9568518e05497d22b8d0be00007346be878f
---
>     sha256: 1c4e3cdada01c16c0fc0518cf03b65ff9e03944d038a5a4f24bc55dec9002c2d
```

One line, and it is `meta.inputs[].sha256` — provenance, which *should* differ
once the inputs changed. The model body is byte-identical. The six
`[[model]]`-declared variants likewise regenerate under their declared `out`
names.

**The eighth, `rust_system_model.yaml`, is an orphan.** Its input no longer
exists. `3f25803d1` added `launch/rust_system.launch.xml`; `9748f7ae3`
("drop the duplicate rust-only launch — two sessions fixed it at once") deleted
it, because a second session had independently added `rust_safety.launch.xml`
for the same purpose (`e93c99483`). The launch file went; the model that had
been scooped in beside it did not. Today the derive rule produces
`rust_safety_model.yaml` from the surviving launch, and nothing produces
`rust_system_model.yaml` at all.

So this is two overlapping parallel-session edits, one of which left a generated
file with no producer.

## Fix

Delete all eight. Seven are reproduced by `nros sync` from inputs that are
already committed (`system.toml` + the launch files); the eighth should never
have existed and has no input to reproduce it from.

Nothing needs authoring in `system.toml` — the usual W4.a remedy — because no
hand-edited content is involved here. This is purely un-committing build output.

## Also worth noting

`nros sync` in that workspace reports a probe failure while still exiting
`done`:

```
gmake: *** [Makefile:150: probe_cpp_safety_listener_pkg__cpp_safe_listener] Error 2
```

The models still generate, so it is not blocking this fix, but a sync that
prints a `gmake` error and reports success is its own defect — a caller cannot
tell a degraded run from a clean one. Worth splitting into its own issue if it
reproduces on a clean tree.

## Prevention

The gate caught this within a day, which is the system working. The residual
gap is upstream of it: a blanket `git add` still scoops generated files into an
unrelated commit, and the author sees only a green build. CLAUDE.md already
carries the rule ("Never `git add -A` / `git add .`"); this is the second time
in the same phase pair that it was the proximate cause.

## Resolution (2026-08-04)

All eight deleted; `check-no-tracked-models` is green.

They were mine: `3f25803d1` added `system_model.yaml` alongside a `[[model]]`
declaration while fixing the zephyr safety entry, and the rest were the same
commit's regenerated siblings — landed just before phase-330 W4 made a tracked
model the defect rather than the norm.

Deleting them was safe only once `nros::main!` stopped requiring a pre-resolved
model (`2b022c32a`): it now resolves from `system.toml` + the launch file, so the
workspace builds with no committed artifact at all.
