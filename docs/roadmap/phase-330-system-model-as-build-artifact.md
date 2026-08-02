# phase-330 — the SystemModel becomes a build artifact (implement RFC-0063)

**Implements:** [RFC-0063](../design/0063-system-model-is-a-build-artifact.md)
**Closes:** issue 0380 (and retires its transitional guards)
**Touches:** phase-296 (model as sole plan input), issue 0320 (content-addressed
staleness), RFC-0047 / RFC-0052 (scheduling dims + fail-loud contract)

**Status.** IN PROGRESS — W1 landed 2026-08-02 (wave 1).

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

- [x] **W1.a** Extend the system-config schema (`nros_orchestration_ir`) with
      per-platform scoped tier dims: `zephyr.deadline_us`,
      `nuttx.budget_us`/`period_us`, `threadx.preempt_threshold`/
      `time_slice_us`, per-platform `core`, and the generic `class`. The
      resolver already carries `posix.core` / `sched_class` — this widens the
      same table rather than inventing a second one.
- [x] **W1.b** ~~Teach the resolver to read them~~ — **NOT NEEDED.** The
      resolver parses this same `[tiers.*]` block into the shared
      `ros_launch_manifest_sched::TierPlatformSpec`, which has carried `core` /
      `deadline_us` / `budget_us` / `period_us` / `time_slice_us` all along.
      The gap was entirely on the nano-ros side: two mirrors of one concept
      drifted, and the NARROWER one (`nros_orchestration_ir::TierRtosSpec`)
      defined what users could write. No fork change, so no maintainer push is
      in the critical path — the risk this phase recorded does not apply.
- [x] **W1.c** Round-trip acceptance, using the tooling issue 0380 already
      landed: for `ws-realtime-rust`, delete the committed model, re-resolve,
      and assert `nros ws model-dims` returns the SAME 20 dims. Equivalently:
      `nros sync` must not refuse, because there is nothing left to drop.
      **This is the falsifiable definition of "the inputs can express it".**
      **PASS (2026-08-02)** for `ws-realtime-rust`: dims moved into
      `system.toml`, model deleted, `nros sync` exits 0 (nothing to drop) and
      `nros ws model-dims` returns the same 20 dims.

### W2 — Prove the round-trip across the family

**The census is bigger than "the realtime workspaces".** Measured 2026-08-02:

| Set | Count |
| --- | --- |
| Tracked `*/config/*model.yaml`, repo-wide | **120** |
| …in `examples/workspaces/` | 80 (36 `system_model.yaml` + 44 variants) |
| …in `packages/testing/.../fixtures/` | 40 |
| Models carrying `execution.tiers` dims | **11** (86 dims) |

The 11 dim-carrying models are the *migration risk*; the other 109 are the
*migration work*. Both have to move — every one of them is a derived artifact
committed under `src/`.

- [ ] **W2.a** Round-trip W1.c across the 11 dim-carrying models: the nine
      `ws-realtime-*` workspaces plus the two `orchestration_tiers_{native,
      freertos}` test fixtures. `scripts/model-dims-baseline.txt` is the oracle.
- [x] **W2.b** Round-trip the remaining models — **DONE (2026-08-02).** All
      **121** committed models regenerated and the diffs classified. **Zero**
      failed to regenerate.

      | Class | Count |
      | --- | --- |
      | byte-IDENTICAL | **117** |
      | drops `target:` (issue 0356) | 1 |
      | provenance only (`sha256`/comments) | 1 |
      | ADDS `meta.inputs` provenance | 2 |

      **The answer: regeneration never loses data. Every difference is the
      COMMITTED copy being stale**, from three distinct causes:

      1. `ws-realtime-cpp` — its deploy layer still carries pre-0356
         `target: linux` because commit `07025368b` RESTORED it by grafting the
         block out of git history (the #380 rescue) rather than re-resolving.
         Hand-restoration reintroduced old content.
      2. the two `orchestration_tiers_*` fixtures — last touched 2026-07-24,
         before `meta.inputs` provenance was emitted, so regeneration ADDS it.
      3. `ws-realtime-cpp-mps2` — `sha256`/comment churn only.

      Omitting `target` is the DOCUMENTED contract, not a loss: `Deploy.target =
      None` means board-agnostic — "a multi-board system runs the same nodes on
      every board, so the consuming entry's own board decides (nano-ros issue
      0356)". Both workspaces declare one `kind = "self"` plus FOUR
      `kind = "embedded"` blocks, which is exactly that case. The fork history
      confirms it is deliberate: `69c13d2 chore: bump rlm — multi-board
      placement is board-agnostic (nano-ros #356)`.

      **A correction to the W2.a note above:** the earlier report that
      `ws-realtime-c` drops `target` was produced by the STALE resolver. With
      the rebuilt one it is byte-identical. Two of this phase's observations
      have now been distorted by that stale binary — re-measure after
      `just setup-launch-resolve`, not before.

      **So `resolve(inputs)` is total for the structure and deploy layers.** The
      only thing regeneration could not reproduce was the execution dims, which
      W1 fixed. That is the premise RFC-0063 needs, now measured rather than
      assumed.

**W2.a is complete at 11/11.** The sweep regenerated every dim-carrying model;
the four non-identical diffs touch `target:`/`sha256`/comments/`inputs` only —
no `execution.tiers` line moved in any of them.
- [ ] **W2.c** Any dim that cannot round-trip is a W1 schema gap, not an
      exception to grant. Record it; do not special-case it.

**W2.a status (2026-08-02): 11 of 11 — see W2.b's sweep, which supersedes this.**
`ws-realtime-rust` (wave 1), `-c`, `-c-mps2`, `-cpp-fvp`, `-cpp-rclcpp`,
`-cpp-subnode`, `-cpp-subnode-portable`. Not yet run: `ws-realtime-cpp`,
`-cpp-mps2`, and the two `orchestration_tiers_*` fixtures.

The dim gap was far smaller than assumed: **9 of 11 already carried their dims
in `system.toml`**; only 5 dims across 2 workspaces needed authoring, and they
were exactly the fields W1.a enabled.

Two findings:

1. **The mps2 failures were a STALE RESOLVER, not a schema gap.** They failed
   with "node '/ctrl_node' is not placed — with multiple [deploy.*] blocks every
   node needs a `nodes = [..]` entry", yet the fork's source already contains the
   fix, in a comment naming `ws-realtime-c-mps2` and `ws-realtime-cpp-mps2`
   explicitly. `just setup-launch-resolve` rebuilt it and `-c-mps2` passed. This
   is issue 0363 C's lagging-resolver class, live.

2. **Regeneration is not byte-identical, and the committed models look STALE.**
   Re-resolving `ws-realtime-c` and `-cpp` drops `execution.deploy.<node>.target:
   linux` (and the long doc comments, which is expected — those moved into
   `system.toml` with the dims). The dim gate does not see this: it watches
   `execution.tiers` only. Either the committed copies predate a resolver
   placement change, or regeneration loses deploy content — W2.b must answer
   which BEFORE W4 deletes anything. Restored to committed state pending that
   answer, rather than folded into an unrelated commit.

### W3 — Relocate the artifact

- [ ] **W3.a** Decide the location. This is the phase's central open question,
      and it is wider than the model — **the model is not the only derived
      artifact living in `src/`**:

      | Artifact | Today | Tracked? |
      | --- | --- | --- |
      | `<pkg>/config/*model.yaml` | in `src/` | **committed** ✗ |
      | `<pkg>/metadata/*.json` | in `src/` | gitignored |
      | `<pkg>/generated/<msg crate>/` | in `src/` | gitignored |

      Two of the three are already gitignored, so the pattern is HALF-MIGRATED:
      the model is simply the one that got committed. Whatever `build/` means
      should mean the same thing for all three, or the next artifact repeats
      issue 0380.

      Options:

      1. **Workspace-level `build/<pkg>/…`** — matches colcon, matches the
         framing that motivated this RFC. One root to delete.
      2. **Per-package output subtree** — smallest diff, keeps relative paths
         short, but leaves derived output interleaved with sources, which is
         the shape being retired.

      **`generated/` is what makes option 1 non-free**: each leaf's
      `.cargo/config.toml` redirects msg crates by RELATIVE path
      (`std_msgs = { path = "generated/std_msgs" }`, RFC-0048 W9, `nros
      sync`-managed). Moving the tree rewrites 110 redirects and lengthens
      every one; issue 0378 is a live reminder that when those redirects are
      wrong, resolution falls through to a third party's crate on crates.io.
      Sequence `generated/` deliberately, or scope W3 to the model and leave
      the other two where they are with a recorded reason.
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

> **Coordinate with phase-331 (RFC-0066) — added 2026-08-02.** That phase folds
> 18 themed workspaces into the four large ones and deletes them. **29 of the
> 120 models below live inside those 18**, plus 10 workspace-root
> `CMakeLists.txt`. Let phase-331 W2–W3 land FIRST: the census then drops to 91
> and this phase does not migrate files that are about to be deleted. Nothing
> here blocks phase-331 — W1 is already landed, and none of the 18 folded
> workspaces carries an `execution.tiers` dim, so their re-resolve is free of
> the issue-0380 hazard.
>
> **Do not run W3–W4 between phase-331's W1 baseline and its W5 re-measure**:
> moving model generation moves fixture build wall-clock, and W5's delta would
> then mix two causes.

- [ ] **W4.a** Remove all **120** tracked `*/config/*model.yaml` (80 under
      `examples/workspaces/`, 40 under test fixtures); add the build location
      to `.gitignore`.
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

- [ ] No tracked `*/config/*model.yaml` remains (120 today).
- [ ] A clean checkout builds every `ws-realtime-*` workspace and the generated
      models carry all 86 dims (`nros ws model-dims` against the retired
      baseline as the oracle).
- [ ] Deleting a build directory and rebuilding reproduces byte-identical
      models — the property that makes it a cache.
- [ ] The realtime e2e family (the ~17 tests that lost their subject in 0380)
      passes against generated models.
- [ ] A copied-out standalone example still builds (W3.c).

## Risks

- **Phase-331 is folding examples underneath this phase.** See the note on W4:
  order its W2–W3 before this phase's W4, and keep this phase's W3–W4 outside
  its W1→W5 measurement window. RFC-0065's builder would remove most of the
  per-move churn that phase pays, but it is a Draft and phase-331 should not
  wait on it.

- **The proc-macro path (W3.b).** `nros-macros` reads the model at expansion
  time. If it cannot see the build directory, either the macro stops reading
  the model or the model must also exist somewhere the macro can reach — which
  would partly reintroduce what this phase removes.
- **The fork boundary (W1.b).** The schema change lands in a vendored fork the
  agent may not push; sequencing needs the maintainer.
- **Nine workspaces at once (W2).** The dims differ per workspace, so W1's
  schema must be complete before the family migrates, or 0380 recurs at scale.
