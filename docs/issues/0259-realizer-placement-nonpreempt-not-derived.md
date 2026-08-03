---
id: 259
title: "Derived scheduling is quantitatively inert — no WCET in the model, so blocking is unmodelled and budget/placement/non_preempt can't be derived"
status: open
type: limitation
area: orchestration
related: [phase-296, rfc-0052, 0403, 0404]
---

## Original finding (phase-296 W5.5–W5.11, 2026-07-24)

`realize_rtos` (`packages/core/nros-orchestration-ir/src/rtos_realizer.rs`)
derives `activation`, `urgency`, `deadline` and `budget` from the self-derived
DAG facts, but `placement` and `non_preempt_scope` are HARDCODED
(`rtos_realizer.rs:347`):

```rust
// non_preempt_scope + placement: not derived from the model yet.
let preempt_real = DimRealization::NotRequested;
let placement_real = DimRealization::NotRequested;
```

So `RealizedNode.core` and `.preempt_threshold` are always `None` — the derived
schedule NEVER assigns a core pin or a preemption threshold. The board
consumers for both dims EXIST and are e2e-verified (W5.7/W5.8 Zephyr core-pin,
W5.9 NuttX sporadic, W5.10 ThreadX preempt-threshold, W5.11 NuttX/FreeRTOS
core-pin) — but they only fire from EXPLICIT per-tier knobs
(`<platform>.core`, `<platform>.preempt_threshold`). The derived path
(`derive_execution_from_contracts`, RFC-0052's whole point) can never produce
them, so a self-derived schedule silently omits placement + non-preemption.

## Rewritten framing (design review, 2026-07-26)

The two hardcoded dims are a SYMPTOM. Tracing the consumers turned up three
findings that reframe the work.

### 1. The blocker is a missing NUMBER (WCET), not a missing adjective

Both remaining dims — and one of the four "working" ones — need per-callback
execution time, and **the model carries none**. `MapperPath.exec_ms` is `None`
everywhere (`chain_aware_mapper.rs:692/701/712`; W5.1 derives it as `None`),
and the realizer's budget arm short-circuits on `budget_ms: None →
NotRequested`. So today:

- **budget** is nominally implemented but practically inert (no WCET ⇒ no
  budget ⇒ no reservation/sporadic on the derived path).
- **blocking** (`B_i`) can only ever be structural ("A can block B"), never
  numeric.
- The feasibility check therefore assumes `B_i = 0`, which is **unsound
  whenever callbacks share a resource** — it reports more headroom than
  exists. This is the most serious item in this issue.

Response-time analysis needs
`R_i = C_i + B_i + Σ_{j∈hp(i)} ⌈R_i/T_j⌉·C_j`; we have neither `C_i` nor `B_i`.

**Action: add per-callback WCET as a measured numeric fact** (node-manifest
level — see §3). It is the highest-value missing input in the whole scheduling
story, and exactly the kind of fact the contract vocabulary already prefers:
numeric, measurable, falsifiable.

### 2. `placement` and `non_preempt_scope` are MECHANISMS, not requirements

The four working dims each trace to a fact that implies them
(`max_latency_ms` → deadline, contracted rate → period, WCET → budget, rank →
priority). "Pin to core N" and "don't preempt me" imply nothing on their own —
they are *ways of achieving* a deadline under interference. Asking "which
contract fact means core-pin?" is the wrong question; the right one is "what
does the realizer need in order to DECIDE placement?" — an interference model,
not a new per-node adjective.

Consequences:

- **placement** splits in two. *Performance placement* (spread load, colocate a
  chain) must be a DERIVED allocation output — from per-core utilization and
  chain structure — never a declared field; the user cannot state it correctly
  because it depends on the whole taskset. *Hardware locality* ("this callback
  services the IMU whose IRQ lands on core 0") is a legitimate declared fact,
  but it names a DEVICE, not a core number, and resolves against the board.
- **non_preempt_scope** derives from resource contention:
  `ceiling(R) = max urgency among R's holders` → ThreadX preempt-threshold
  natively, executor-enforced Backfill elsewhere, Degrade recorded otherwise.
  The same contention set yields `B_i`.

### 3. Contention is already declared — and does NOT belong in the contract layer

`CallbackGroupDecl { type: "MutuallyExclusive" }` already states "these
callbacks never run concurrently" — a resource declaration in all but name,
authored by the node author, carried in `execution.bindings`, enforced by the
executor. For the intra-node case (the case that exists in-tree today) NO new
vocabulary is needed: `B_i` = longest WCET among the group's other members;
`ceiling` = max urgency in the group.

A `holds: [resource]` field in `contracts.node_paths` was considered and
**rejected**: a contract is an interface promise (what other components may
rely on), while locking is an implementation fact — invisible at the interface,
changed by refactors, and different between two implementations of the same
interface. It belongs in the node's own manifest beside the callback groups.
Cross-node hardware contention is then INFERRED: two nodes whose manifests
both bind `spi0`, plus a board that has one `spi0`, contend. Nobody writes a
contention contract.

If a resource name ever does become referenceable, it must be a first-class
declared entity with a resolution check (like topics) — a free-form string is
worse than nothing here, because a typo yields NO match, hence no blocking
term, hence a MORE optimistic verdict. Silence on a misspelled safety-relevant
input is the wrong failure mode.

Rejected outright: deriving placement from `criticality`. It is an ordinal
adjective with no unit, unfalsifiable, already load-bearing as a mapper
tie-break, and imported from the assurance-process world (DAL/ASIL). Making it
imply core pinning would (a) fragment capacity with no schedulability
argument, (b) change physical topology as a side effect of an "importance"
label, and (c) resemble a spatial-partitioning claim that nothing here backs
(no cache/bus interference bound). Where deadlines exist, deadline-monotonic
ordering already determines priority; where they do not, the honest response is
a warning ("unconstrained path — declare a deadline or rate"), not a silent
bucket. Note the present hazard: a tight-deadline path marked `criticality:
low` currently sorts BELOW a slack path marked `high` — priority inversion by
adjective.

## Re-verified 2026-08-03 — every claim still holds

Checked against the current tree, not assumed from the write-up:

* `rtos_realizer.rs` still hardcodes both dims — its own module doc now states
  it: "`non_preempt_scope` and `placement` are `NotRequested`".
* **`exec_ms` is `Some(..)` in TEST CODE ONLY** (`rtos_realizer.rs:514`,
  `sched/src/chain.rs:336/379`). Nothing on the production path populates a
  WCET, so the budget arm is inert exactly as described.
* The unsoundness is visible in one line of `chain_aware_mapper.rs`:

  ```rust
  ChainElement::Boundary { period_ms, exec_ms, .. } => Some(period_ms + exec_ms.unwrap_or(0.0)),
  ```

  A missing WCET is counted as **zero cost**, there is no blocking term, and
  the result is a bare `feasible: bool` that discloses neither assumption. So a
  chain is reported feasible on the strength of inputs nobody supplied.

**A cheap step exists that needs none of the three prerequisites:** make the
verdict stop overclaiming. `ChainFeasibility` could record that it assumed zero
execution time (and for which elements), surfaced through `meta.diagnostics` the
way unmatched components already are. That does not make the analysis sound — it
makes it honest about what it did not model, which is the difference between "no
headroom problem" and "no headroom evidence". Worth doing before, and
independently of, staged step 1.

## Prerequisites (blocking, in order)

1. **Per-callback WCET** in the node manifest (measured). Without it, budget +
   blocking stay inert and the feasibility verdict stays optimistic.
2. **Board peripheral registry** — `BoardDescriptor` today is build-oriented
   (toolchain / features / link kind) with no device list; both cross-node
   contention (`spi0`) and hardware locality (`drives: imu0`) need one.
3. **`SchedCaps` core count** — `affinity: bool` exists, but placement
   allocation needs the number of cores.

## Direction (staged; each step lands green on its own)

1. **Ceiling + blocking from callback groups** — zero new user vocabulary.
   Derive `ceiling(R)` / `B_i` from MutuallyExclusive group membership, feed
   `B_i` into the feasibility check (fixes the unsoundness for the intra-node
   case), and fill `preempt_real` through the existing Native/Backfill/Degrade
   machinery → `RealizedNode.preempt_threshold`. Quantitatively meaningful only
   once (1) lands; structurally correct before that.
2. **Hardware locality** — device bindings in the node manifest + the board
   peripheral registry → `placement_real` where the board routes the IRQ,
   fail-loud when it cannot.
3. **Performance placement** — per-core utilization + chain colocation as an
   allocation OUTPUT, rendered in `--explain` ("chain A → core 1: util 0.62,
   segments s1→s3 colocated") and Degrade-marked honestly ("affinity from
   utilization only; no cache/bus interference bound proven").
4. **Emit the resulting knobs** from `derive_execution_from_contracts` into the
   synthesized `[tiers.*]` rows so the existing board consumers fire on the
   derived path.

## Step 1 landed 2026-08-03 — the verdict no longer overclaims

The cheap step above is done, in `ros-launch-manifest` v0.1.4 (rlm commit
`234a005`), and consumed here at that tag.

`ChainFeasibility` now carries `boundaries_without_wcet`, and
`chain_feasibility()` records each boundary it counted as zero instead of
summing through a `filter_map`. A chain judged FEASIBLE on any such boundary
raises `MapWarning::ChainFeasibleWithoutWcet`, naming them; layer 2 renders it
in the pipeline's `"scheduling: ..."` style and, like `ChainInfeasible`, never
treats it as fatal — an evidence gap is not a band-fit violation, so it stays
collect-only even in Strict mode.

Infeasible verdicts are deliberately left alone: a chain that fails to fit even
at zero execution time fails for real, and that conclusion needs no evidence it
does not have.

The arithmetic is unchanged — `unwrap_or(0.0)` is still there, because the
alternative is inventing a WCET, which is a different lie. What changed is that
the assumption is now stated wherever the number is consumed. Notably, rlm's
own design-doc worked example trips the warning (`/ekf/p_ekf`,
`/planning/p_planning` carry no WCETs): it has always ranked chains on partly
absent evidence and read as if it had not.

**This closes nothing.** It makes the gap loud so the remaining work is
visible, and splits into two follow-ups:

* **0403** — the producer. `nros-bench/wcet-cycles-qemu` emits prose nothing
  parses, and on QEMU (dead DWT) prints zeros and exits 0.
* **0404** — the schema. There is still no syntax anywhere for a developer to
  declare a WCET they measured, so the new warning currently reports a problem
  with no remedy.

The three prerequisites (a WCET source, a blocking model, a disclosed verdict)
are unchanged for the quantitative work below; only the third is now partly
met.

## Open question for RFC-0052

The executor runs one task per tier, so a ceiling derived per callback GROUP
may be unrepresentable at runtime — in which case the honest realization is
tier-level with a Degrade record ("ceiling applied at tier granularity;
group-level non-preemption not enforced"). Decide group-granular vs
tier-granular before implementing step 1.
