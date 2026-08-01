# phase-330 — the SystemModel becomes a build artifact (implement RFC-0063)

**Implements:** [RFC-0063](../design/0063-system-model-is-a-build-artifact.md)
**Closes:** issue 0380 (and retires its transitional guards)
**Touches:** phase-296 (model as sole plan input), issue 0320 (content-addressed
staleness), RFC-0047 / RFC-0052 (scheduling dims + fail-loud contract)

**Status.** OPEN — not started.

## Goal

The committed `config/system_model.yaml` disappears. The model is generated per
build into `build/` in the colcon manner, readable for inspection, authored by
nobody. The user maintains three inputs and only these: the **launch file**, the
**project config**, the **system config**.

## The one ordering constraint that matters

The committed models carry hand-authored `execution.tiers` dims that the inputs
**cannot currently express**. So:

> **The inputs must gain the dims BEFORE any committed model is deleted.**

Reverse that order and the migration reproduces issue 0380 across nine
workspaces at once instead of two. W1 gates everything after it; there is no
useful partial progress on W3–W5 without it.

## Work items

### W1 — Give the dims a resolver input (BLOCKS EVERYTHING)

- [ ] **W1.a** Extend the system-config schema (`nros_orchestration_ir`) with
      per-platform scoped tier dims: `zephyr.deadline_us`,
      `nuttx.budget_us`/`period_us`, `threadx.preempt_threshold`/
      `time_slice_us`, per-platform `core`, and the generic `class`. The
      resolver already carries `posix.core` / `sched_class` — this widens the
      same table rather than inventing a second one.
- [ ] **W1.b** Teach `ros-launch-resolve`'s `sched_loader` to read them and
      emit them into the resolved model. NOTE: that is a **vendored fork**
      (`packages/cli/third-party/ros-launch-resolve`) — per CLAUDE.md the agent
      commits and rebases there but does not push the fork remote; the
      maintainer pushes, then the superproject pointer moves.
- [ ] **W1.c** Round-trip acceptance, using the tooling issue 0380 already
      landed: for `ws-realtime-rust`, delete the committed model, re-resolve,
      and assert `nros ws model-dims` returns the SAME 20 dims. Equivalently:
      `nros sync` must not refuse, because there is nothing left to drop.
      **This is the falsifiable definition of "the inputs can express it".**

### W2 — Prove the round-trip across the family

- [ ] **W2.a** Repeat W1.c for all nine `ws-realtime-*` workspaces plus the
      subnode/portable variants — 86 dims across 11 models today
      (`scripts/model-dims-baseline.txt` is the census).
- [ ] **W2.b** Any dim that cannot round-trip is a W1 schema gap, not an
      exception to grant. Record it; do not special-case it.

### W3 — Relocate the artifact

- [ ] **W3.a** Decide the location (RFC-0063 open question):
      `build/<pkg>/system_model.yaml` mirrors colcon most closely; a single
      workspace-level `build/nros/` is the alternative.
- [ ] **W3.b** Update the consumers. Ten files resolve the path today:

      | Consumer | Why it is awkward |
      | --- | --- |
      | `cmd/ws.rs`, `cmd/plan.rs`, `cmd/codegen_system.rs`, `cmd/codegen.rs` | ordinary CLI paths |
      | `nros-build/src/lib.rs` | build-script library — runs before the build dir is conventional |
      | `nros-macros/src/main_macro.rs` | **a proc-macro** reads the model at expansion time; it has no build-dir context |
      | `cmake/NanoRosEntry.cmake` | cmake must agree with the CLI on the location |
      | `scripts/build/compile-check-fixtures.sh` | fixture staging |
      | `examples/workspaces/ws-realtime-rust/src/threadx_entry/src/main.rs` | an EXAMPLE source references it |

- [ ] **W3.c** Standalone copy-out examples (CLAUDE.md: no workspace walk-up)
      must still work: a copied-out example has to generate its own model from
      its own inputs, which means the resolver must be reachable from a
      copied-out tree or the example must ship a pre-generated model. Decide
      which; this is the constraint most likely to force a change to W3.a.

### W4 — Delete the committed models

- [ ] **W4.a** Remove every tracked `config/system_model.yaml`; add the build
      location to `.gitignore`.
- [ ] **W4.b** Update issue 0320's staleness text: a per-build artifact does not
      need `meta.inputs[]` sha256 provenance any more than an object file needs
      it. Decide whether `meta.inputs` stays for inspection or goes.

### W5 — Retire the transitional guards

- [ ] **W5.a** Remove the `nros sync` dim-loss refusal (`prior_model_dims` +
      the drop check) — it protects a committed file that no longer exists.
- [ ] **W5.b** Remove `check-model-dims`, `scripts/model-dims-baseline.txt` and
      the `check-fast` wiring. Keep `nros ws model-dims`: inspection is a
      REQUIREMENT of RFC-0063, not a leftover.
- [ ] **W5.c** Do W5 in the SAME change as W4. A guard removed early leaves the
      models unprotected; a guard left behind fails on files that no longer
      exist.

### W6 — Documentation

- [ ] **W6.a** Book: where the model lands, how to read it, that it is output.
- [ ] **W6.b** CLAUDE.md: the model line currently implies a committed file.
- [ ] **W6.c** Mark RFC-0063 `Stable` and close issue 0380.

## Acceptance

- [ ] No tracked `system_model.yaml` remains.
- [ ] A clean checkout builds every `ws-realtime-*` workspace and the generated
      models carry all 86 dims (`nros ws model-dims` against the retired
      baseline as the oracle).
- [ ] Deleting a build directory and rebuilding reproduces byte-identical
      models — the property that makes it a cache.
- [ ] The realtime e2e family (the ~17 tests that lost their subject in 0380)
      passes against generated models.
- [ ] A copied-out standalone example still builds (W3.c).

## Risks

- **The proc-macro path (W3.b).** `nros-macros` reads the model at expansion
  time. If it cannot see the build directory, either the macro stops reading
  the model or the model must also exist somewhere the macro can reach — which
  would partly reintroduce what this phase removes.
- **The fork boundary (W1.b).** The schema change lands in a vendored fork the
  agent may not push; sequencing needs the maintainer.
- **Nine workspaces at once (W2).** The dims differ per workspace, so W1's
  schema must be complete before the family migrates, or 0380 recurs at scale.
