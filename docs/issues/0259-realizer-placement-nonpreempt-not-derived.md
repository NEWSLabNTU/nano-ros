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

## Partly addressed 2026-08-19 — `B_i` derived; the ceiling is a TAUTOLOGY today

RFC-0078 gave callbacks a way to carry WCETs, which unblocked the numeric half
of this issue. One of the two terms named in finding 3 is now derived; the other
turns out to be a no-op at the current model granularity, and that is the more
useful finding.

### Derived: the blocking term `B_i`

`RealizedNode.blocking_us` — the longest a callback on this node can wait for a
mutually-exclusive sibling. A node's callbacks are serialised within their tier
task (v1 treats every `CallbackGroupDecl` as mutually exclusive), so a ready
callback waits for whichever sibling is running, and the worst case is the
longest sibling: the SECOND-largest WCET on the node, since the longest callback
cannot block itself.

`None` when fewer than two callbacks carry a WCET, and `None` means NOT
DERIVABLE rather than "no blocking". A feasibility check that reads it as zero
reproduces exactly the optimism this issue is about — the same distinction
`ChainFeasibleWithoutWcet` draws upstream.

### NOT derived, and deliberately: the preemption threshold

Finding 3's rule is `ceiling = max urgency among the group's members`. Over a
node's own callbacks that is, today, **the node's own priority** — by
construction:

* `dense_node_ranks` assigns each node the MINIMUM (best) rank among its path
  items;
* `rank_to_priority` turns that into the task priority.

So `ceiling(node's callbacks) == priority(node)`, and emitting
`preempt_threshold = ceiling` would set the threshold equal to the priority for
every node in every system. It would satisfy "a dim is derived" while saying
nothing — which is what this issue's own acceptance warns against ("do not close
0259 on a schema alone").

A preemption threshold earns its keep only when callbacks **inside one task hold
different priorities** and a lower one must not be preempted by a higher
sibling. The emitted plan collapses each node to a single priority, so that
situation is not currently representable.

**What non_preempt actually requires**, then, is not a realizer change but a
model change: per-callback priorities within a tier, carried through
`RealizedNode` and the emitted `TierDef`, plus a runtime dispatch that honours
them. That is the prerequisite, and it should be scoped as its own work item
rather than attempted inside the realizer.

### `B_i` has a consumer: a necessary-condition feasibility check

Added the same day. If a node's own worst callback plus the longest sibling it
can wait for already exceeds its deadline, no priority assignment, core pin or
preemption threshold can rescue it — interference from other nodes only adds. So
`C_i + B_i > D_i` is reported as a `Degradation { dim: "feasibility" }`, which
both existing consumers (codegen-system and the `nros::main!` macro) already
print, so it surfaces with no new plumbing and cannot be silently dropped.

The reason string carries the inputs — `C=6000us + B=5000us > D=10000us` — so
the verdict is auditable rather than an assertion.

**Deliberately not full response-time analysis.** RTA needs
`Σ over higher-priority tasks ceil(R/T)*C`, and with most callbacks carrying no
WCET that sum would be missing terms: an optimistic number presented as an upper
bound, which is precisely this issue's failure. A necessary condition can only
MISS an infeasible node; it cannot invent one.

Two silences are correct and tested: no WCET means no verdict (an undeclared
execution time is not a claim that the node fits), and no deadline means nothing
to exceed. Where `B_i` itself is unknown it is counted as 0 for this check ONLY
— that biases toward silence, the safe direction for something that stops a
build — and the report says `unknown (counted as 0)` rather than presenting an
absent term as a measured zero.

### The verdicts reach the artifact, and `nros explain` shows them

Routed the same day. `nros-plan.json` gains an additive `sched_warnings` array
(`{ node, dim, reason }`), written by `codegen-system` — the command that both
derives the schedule and writes the plan, so no cross-verb plumbing was needed.
`nros explain` renders them ABOVE the SchedContext table: a reader who is told
"this node's declaration is infeasible" should see that before the priorities
derived for it, not as a footnote after them.

Why the artifact and not just stderr: a warning printed at bake time is gone
when the terminal scrolls, and the plan is what anyone inspects afterwards. A
verdict that exists only in scrollback cannot be audited, which defeats the
point of making the check carry its inputs.

Additive and omitted when empty, so a system that derives nothing produces a
byte-identical plan and byte-identical `explain` output. The test builds its
plan by DESERIALISING one rather than constructing the struct, so it fails if
the field ever stops round-tripping through the schema.

### Derived: system utilisation — and why it stops at a uniprocessor

`U_i = C_i / T_i` is the fraction of a processor a periodic task needs. Summed,
it is a NECESSARY condition in the same family as the per-node check: a taskset
demanding more than one processor cannot run on one, whatever the priorities.
Reported as `Degradation { node: "<system>", dim: "utilization" }` with the
percentage, so it lands in `nros-plan.json` and `nros explain` alongside the
rest.

Three deliberate restraints:

* **Only reported when it EXCEEDS capacity.** A utilisation that fits says
  nothing on its own — the rate-monotonic bound is `n(2^(1/n)-1)`, not 1.0 — so
  this makes no schedulability claim below 100%.
* **Unmeasured nodes are NAMED in the message.** A total under 1.0 must not read
  as "the system fits" when half the taskset contributed nothing for want of a
  WCET. The verdict says how many were skipped and that the real total is
  higher.
* **Silent on SMP.** See below.

### Why `placement` cannot be derived at all: nothing counts the cores

`SchedCaps` carries `affinity: bool` and **no core count**, and neither does
anything else in the tree (`grep -rn "n_cores\|core_count\|num_cores"` over
`packages/core` and `packages/boards` returns nothing). Finding 2 asks the
realizer to derive performance placement from per-core utilisation — but there
is no denominator to divide by, and no set of cores to assign to.

So `placement` is not blocked on an algorithm; it is blocked on a board fact
nobody records. The utilisation check above therefore stays silent on an SMP
board rather than guessing a core count, which is the same restraint for the
same reason.

**What placement needs first:** a core count in the board descriptor, reaching
`SchedCaps`. Until then any placement derivation would be inventing its own
hardware model.

#### Added 2026-08-19 — `SchedCaps.n_cores`, declared per deployment

`[deploy.<board>] cores = <n>` — the same bake-authoritative knob shape as
`edf`, because the deployment is the only place that knows and it is
authoritative for the image actually being built. A non-positive count is
IGNORED rather than trusted: zero cores is a typo, not a claim, and honouring it
would divide by nothing.

`n_cores` is `Option`, and `None` means UNKNOWN rather than one. No board
descriptor records a count and a bake cannot infer one — assuming 1 reports
false over-subscription on an 8-core host, assuming many excuses a taskset that
cannot fit. Both fabricate hardware, which is the same failure as a fabricated
WCET.

**This also fixed a bug in the utilisation check as first landed.** Its gate was
`!caps.affinity`, and `affinity` is `true` for posix, zephyr, freertos, threadx
AND nuttx — every real target — so the check never fired anywhere. A capability
flag was standing in for a quantity it cannot express. The gate is now the
declared count, and the verdict reads `demand N.NN processors, and this
deployment declares M`.

`placement` remains underived: a count makes utilisation judgeable, but
assigning nodes to cores also needs the interference model finding 2 describes.
What is no longer missing is the denominator.

### Still open

`placement` needs a CORE COUNT before it needs an algorithm (above), and then
the interference model finding 2 describes. `non_preempt` needs per-callback
priorities within a tier. Both are missing model INPUTS rather than missing
realizer logic — which is why neither is a realizer change. The check is a NECESSARY condition, not a sufficient one:
it cannot say a system IS schedulable, only that a declaration is not. Saying
the former needs the taskset model — and WCETs on every callback, which needs
hardware.

Caveat on evidence: on a host with no hardware lane every WCET feeding this is
declared rather than measured (RFC-0078's `SYNTHETIC` caveat), so the derivation
is demonstrably correct and not demonstrably informed.

## Re-verified 2026-08-03 — every claim still holds

Checked against the current tree, not assumed from the write-up:

* `rtos_realizer.rs` still hardcodes both dims — its own module doc now states
  it: "`non_preempt_scope` and `placement` are `NotRequested`".
* **`exec_ms` is `Some(..)` in TEST CODE ONLY** (`rtos_realizer.rs:514`,
  `sched/src/chain.rs:336/379`). Nothing on the production path populates a
  WCET, so the budget arm is inert exactly as described.
  **[STALE as of 2026-08-21 — issue 0404 landed a production path;
  see the prerequisites section.]**
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

**Status re-measured 2026-08-21 — two of the three are now MET.** The list below
is the original; the annotations are what the tree actually holds.

1. ~~**Per-callback WCET** in the node manifest (measured).~~ **MET.** Issue 0404
   shipped the declaration schema (RFC-0078, `7ccfd38c9`) and the production path
   now populates it: `mapper_input.rs:92`,
   `exec_ms: wcet.and_then(|w| w.exec_ms(path_ref))`. The 2026-08-03
   re-verification below still says "`exec_ms` is `Some(..)` in TEST CODE ONLY" —
   that is no longer true. Note what MET does and does not mean here: a
   declaration can now REACH the mapper, so budget and blocking are no longer
   inert by construction. Whether any given tree HAS declarations is a separate
   question, and RFC-0078's `SYNTHETIC` caveat below still applies on a host with
   no hardware lane.
2. **Board peripheral registry** — STILL OPEN. Re-checked: `BoardDescriptor`
   (`nros-cli-core/src/orchestration/board_descriptor.rs:176`) carries names,
   platform, target, toolchain, platform_feature, local_aliases, link_kind,
   entry_kind and netstacks — all build-oriented, no device list. Both
   cross-node contention (`spi0`) and hardware locality (`drives: imu0`) need
   one. This is the only original prerequisite left.
3. ~~**`SchedCaps` core count**~~ **MET.** `caps.n_cores` exists and is consumed
   — set at `rtos_realizer.rs:239`, and the utilisation denominator reads it at
   `:524`.

### What that leaves, stated so the list is not read as "one step away"

Neither remaining dim is unblocked by the two that were met, and neither is a
realizer change — which is this issue's own repeated finding:

* **`non_preempt`** is blocked on a MODEL change, not on a prerequisite in this
  list: per-callback priorities within a tier, carried through `RealizedNode`
  and the emitted `TierDef`, plus a runtime dispatch that honours them. Until
  callbacks inside one task can hold different priorities, `ceiling(node's
  callbacks) == priority(node)` by construction and emitting a threshold would
  say nothing — see "NOT derived, and deliberately" above. That work item does
  not exist yet.
* **`placement`** is blocked on prerequisite 2, and then on the interference
  model finding 2 describes.

So the honest summary is: the QUANTITATIVE blocker this issue was rewritten
around (finding 1, "the blocker is a missing NUMBER") is gone, and the two
hardcoded dims are still blocked — on different things than the number.

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

## `placement` DERIVED 2026-09-04 — the performance half, now that a core count exists

The section above closed with "what placement needs first: a core count in the
board descriptor, reaching `SchedCaps`". `n_cores` landed the same day, and this
is the derivation it unblocked. `realize_rtos` now assigns `RealizedNode.core`
and sets `placement_real = Native`.

**Only the PERFORMANCE half**, which is the half finding 2 said must be derived
rather than declared. Hardware locality is untouched: it names a DEVICE and
resolves against the board, and this tree has no device vocabulary to name one
with. Chain colocation is untouched too — it needs the interference model
finding 2 describes, not just a count, and inventing one is the same
fabricated-hardware failure as an invented WCET.

Worst-fit decreasing: largest utilisation first, each onto the least-loaded
core. Worst-fit rather than first-fit because the goal is to SPREAD load;
first-fit packs cores tight, which maximises interference on the ones it fills.
Ties break on node name, so two identical bakes produce identical placements
rather than a diff nobody can explain.

### Four preconditions, each refusing rather than guessing

1. **`affinity`** — the board must have the mechanism.
2. **`n_cores >= 2`** — on a uniprocessor every node lands on core 0, which
   satisfies "a dim is derived" while saying nothing. That is exactly the
   tautology this issue rejected for `preempt_threshold`, and it is no better
   here.
3. **Every PERIODIC node measured.** One unmeasured node and the whole
   derivation refuses: a bin-pack over a taskset it cannot see is the same
   fabrication as an invented WCET. Aperiodic nodes contribute no utilisation
   and are left unpinned rather than counted as zero.
4. **The packing must FIT** — see below.

### The finding the aggregate utilisation check cannot make

Three tasks at `U=0.6` on two cores total **1.80** against a capacity of
**2.00**, so the system-wide utilisation check is correctly silent. Yet no core
can hold two of them: partitioned fixed-priority scheduling runs a task on ONE
core, so total headroom does not imply a feasible partition.

That case is now a `Degradation { dim: "placement" }` carrying its inputs, and
NO node is pinned — an infeasible packing assigns nothing rather than a best
effort. It rides the existing `sched_warnings` route into `nros-plan.json` and
`nros explain`, so it needed no new plumbing.

### Verified

Five tests, and the partition one proven by SABOTAGE rather than assumed:
relaxing the fit guard to `worst > 99.0` makes
`a_taskset_that_fits_in_total_but_in_no_partition_is_reported` fail, so it is
testing the guard and not merely passing beside it. Its first assertion pins the
premise — that the utilisation check stays silent on that taskset — so the test
cannot quietly stop exercising the partition case. 100 lib tests green,
`just check fast` green.

## Still open after this

* **`non_preempt`** — unchanged, and still not a realizer change. It needs
  per-callback priorities within a tier, carried through `RealizedNode` and the
  emitted `TierDef`, plus a runtime dispatch that honours them. Its own work
  item, as recorded above.
* **The inputs are SYNTHETIC.** RFC-0078's worked example says so: QEMU cannot
  measure cycles and there is no hardware lane, so no run has produced a real
  `nros.wcet.measurements/1` artifact. The placement machinery is correct and
  will stay silent until something declares a WCET — which is the honest state,
  not a defect, but it means this derivation has never run on measured numbers.
* **Hardware locality** and **chain colocation**, as above.
