# RFC-0063 — The SystemModel is a build artifact, not a committed source

**Status:** Draft (2026-08-02)
**Amends:** phase-296 (committed SystemModel as the sole `nros plan` input),
issue 0320 (content-addressed model staleness), RFC-0047 / RFC-0052 (scheduling
dims and their fail-loud contract).
**Implemented by:** [phase-330](../roadmap/phase-330-system-model-as-build-artifact.md)
**Motivated by:** issue 0380 — two regeneration commits deleted committed
models, stripped 17 hand-authored scheduling dims, and ~17 realtime e2e tests
silently lost their subject.

## Problem

`config/system_model.yaml` is committed, and it holds two kinds of content with
opposite lifecycles:

| Content | Where it comes from | If deleted |
| --- | --- | --- |
| `structure`, `deploy`, `meta.inputs` | `resolve(launch + system config)` | regenerates identically |
| `execution.tiers` dims (`zephyr.deadline_us`, `nuttx.budget_us`/`period_us`, `threadx.preempt_threshold`/`time_slice_us`, per-platform `core`) | **hand-authored in the model** | gone forever |

Two live conventions therefore contradict each other:

1. **Issue 0320** treats the model as a *cache*: it carries `meta.inputs[]`
   sha256 provenance, staleness is detectable, and the documented remedy when
   inputs drift is "delete it and re-resolve".
2. **Phase-296 W5** treats it as a *source of truth*: the dims live there
   precisely because the system config schema cannot express them.

A file cannot be both. Anything hand-authored into a file whose maintenance
procedure is "regenerate from inputs" is data waiting to be deleted — and it
was, twice.

Issue 0380 landed guards that make the loss loud (`nros sync` refuses to shrink
a model; `check-model-dims` diffs a baseline). Those stop the bleeding. They do
not resolve the contradiction: the model is still a committed file that nobody
can safely regenerate.

## Decision

**The SystemModel is a derived build artifact.** It is generated on the fly
when the workspace is built, written under `build/` in the colcon manner, and
exposed for INSPECTION rather than editing.

The user maintains three inputs, and only these:

- the **launch file** — topology and node identity;
- the **project config** — what the workspace is and how it is built;
- the **system config** — deployment and execution properties, including the
  scheduling dims that today live in the model.

This makes the model unambiguously a cache. `resolve(inputs)` becomes total: if
regenerating it can lose information, that is a bug in the input schema, not a
process someone must remember not to run.

## Consequences

**The dims need a home in the system config.** This is issue 0380's
"direction A", and this RFC makes it mandatory rather than one option of three:
per-platform scoped tier dims must be expressible in user-maintained
configuration, or the decision above cannot hold. The resolver already carries
`posix.core` / `sched_class`; this widens the same table.

**Committed models are retired.** Every `config/system_model.yaml` in
`examples/workspaces/` becomes generated output. `.gitignore` gains the build
location; the tracked copies are deleted once the inputs can reproduce them.

**Issue 0320's staleness machinery changes shape.** Content-addressed
provenance exists because a committed artifact can silently disagree with its
inputs. A build-directory artifact regenerated per build cannot: the ordinary
build-system freshness rules apply, the same way object files do not carry
sha256 provenance of their sources.

**Issue 0380's guards become transitional.** The sync-time refusal and
`check-model-dims` protect data that will no longer exist in a committed file.
They should be removed in the same change that removes the last committed
model — not before, and not silently.

**Inspection needs to stay easy.** "Exposed for inspection" is a requirement,
not a side effect: a user debugging placement must be able to read the resolved
model without reverse-engineering it. The `build/` location must be documented
and stable, and `nros ws model-dims` (issue 0380) already reads a model from an
arbitrary path.

## Open questions

- **Config surface for the dims.** Exact schema for per-platform scoped tiers
  in the system config, and whether it lives in `system.toml`'s
  `nros_orchestration_ir` schema or beside it.
- **Build location.** `build/<pkg>/system_model.yaml` mirrors colcon most
  closely; the alternative is a single workspace-level `build/nros/`.
- **Migration.** The committed models carry hand-authored dims that the inputs
  cannot yet express, so the inputs must gain them BEFORE the models are
  deleted, or the migration reproduces issue 0380 at scale.
- **Consumers that read a committed path today.** `nros plan`, the entry
  codegen and the cmake integration resolve the model by convention; each needs
  to follow the build-directory location.

## Non-goals

Changing what the model *contains*, or the fail-loud contract of RFC-0052. This
RFC moves where the model lives and who authors it; the schema of a resolved
system is unchanged.
