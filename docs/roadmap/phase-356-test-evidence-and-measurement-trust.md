# Phase 356 — Test evidence: a sweep you can read afterwards, and numbers you can believe

**Status (2026-08-17). W1 and W2 DONE; W3's available half DONE, its remainder
unblocked and specified.** Three issues about the same failure: a test run that
reports something which cannot be checked, or which is not what it appears to
be.

**Two corrections to the 2026-08-16 status, both found by checking the issues
rather than the phase doc:**

* **W1 was already done.** #527 was resolved 2026-08-15 in phase-340 — the day
  BEFORE this doc recorded it as "not started". `_rewrite-skipped-junit`
  snapshots to `junit-real.xml` and `_name-real-failures` prints the ids. The
  phase doc was simply stale; nothing was owed.
* **W3 was never blocked on phase-162.** This doc said accepting the dims
  "needs capabilities a normal test host does not have". #260 says otherwise
  and is right: sporadic / EDF / preempt-threshold were ALREADY kernel-accepted,
  and `sched_setaffinity` is unprivileged (which is why the posix core-pin
  accept arm exists). What the four remaining arms need is an image with **more
  than one CPU**, which is a fixture question, not a privilege one. The blocker
  was written into this doc, not into the work.

* **W2 (#403)** — DONE, both halves. The bench no longer prints a table of zeros
  from a run whose cycle counter never counted: it refuses and exits FAILURE.
  And it now emits `NROS_WCET_V1` marker records beside the prose, which
  `scripts/bench/wcet-log-to-json.py` lifts into a `nros.wcet.measurements/1`
  artifact. That unblocks [phase-357](phase-357-wcet-as-declared-data.md) W1 —
  #404 has a real artifact to design a schema against instead of guessing.
* **W1 (#527)** — DONE in phase-340 (2026-08-15), before this phase recorded it
  as outstanding. See the correction above.
* **W3 (#260)** — the half that could be done without an SMP fixture is DONE;
  the remainder is unblocked, not phase-162-blocked. See W3 below.

**Owns:** [issue 0527](../issues/0527-doctest-run-overwrites-rewritten-junit.md),
[issue 0403](../issues/archived/0403-wcet-bench-machine-readable.md) (resolved),
[issue 0260](../issues/0260-native-dim-kernel-accept-never-exercised.md).

**Related:** [RFC-0051](../design/0051-test-matrix-architecture.md) (`matrix::CELLS` /
`interop::CELLS`), [phase-329](archived/) (test consolidation),
[phase-296](phase-296-system-model-consumption.md) (names #260),
[phase-357](phase-357-wcet-as-declared-data.md) (the WCET *schema*; #403 is its
measurement side).

## The common shape — silence that reads as success

Each of these produces output that looks like a result and is not one:

* **#527** — the run reports a trustworthy COUNT of real failures, then
  overwrites the evidence for WHICH they were.
* **#403** — a QEMU run with a dead cycle counter reports **zeros as if they
  were measurements**.
* **#260** — the kernel-ACCEPT path is never exercised; only the FALLBACK arm
  is, and the suite passes either way.

This is the same class CLAUDE.md already names in two places: "tests must fail
on unmet preconditions — bare `eprintln!`+`return` reports PASS", and the STALE
verdict being ABSORBING (issue 0445) where a fixture that never launches gets a
message substituted for whatever it would have done. A number nobody can
distinguish from a non-measurement is worse than a missing one.

---

## W1 — The doctest run overwrites the rewritten junit.xml (#527)

`just test-all` (and therefore `ci-matrix`) rewrites `[SKIPPED]` panics to
`<skipped>` in `target/nextest/default/junit.xml` — that rewrite is what makes
the failure count trustworthy, and is why a bare `cargo nextest` counts
`nros_tests::skip!` panics as FAILURES (CLAUDE.md).

The doctest run then writes the same path, destroying the rewritten file. So a
failed sweep reports "Real failures: N" and cannot name them afterwards.

This is the cheapest of the three and has the widest blast radius: it degrades
every future debugging session on a red sweep, including the ones for the other
issues in this phase.

**Acceptance.** After a sweep with at least one real failure and at least one
skip, the junit on disk names the failure AND shows the skip as `<skipped>`.
Verified by forcing both in one run, not by inspection.

## W2 — The WCET bench emits prose nothing parses, and zeros that look measured (#403)

Two defects, and the second is the dangerous one:

1. The bench emits prose no tool consumes, so results cannot be tracked over
   time.
2. A QEMU run with a dead cycle counter reports **zeros** — indistinguishable
   from "this operation took no measurable time".

Fix (2) first and independently: a dead counter must make the run FAIL, not
produce a zero. That is a small change and it stops bad numbers entering any
record, including phase-357's.

~~(1) is a format question and should be settled together with
[phase-357](phase-357-wcet-as-declared-data.md) W1, so the bench emits what the
schema wants to consume rather than a second spelling.~~

**Struck 2026-08-16 — that sentence created a deadlock this phase does not
have.** phase-357 W1 is blocked on #403 emitting an artifact; if #403's format
in turn waited on W1, neither could ever start. I wrote both halves of that
circle.

#403 does not have it. Its own Direction specifies the artifact independently:

> 1. **Emit a structured artifact** — JSON or TOML beside the log, carrying per
>    measurement `min` / `max` / `mean`, `iterations`, and the identity of what
>    was measured. Prose stays for humans; **the artifact is what any future
>    declaration (0404) is generated from.**
> 3. **Record the conditions.** … Cycles convert to the `ms` the mapper wants
>    only through a clock rate, so an artifact without one is not convertible.

The two are different objects. #403's artifact is a MEASUREMENT RECORD — what
was measured, under what conditions, with the counter's validity recorded so a
stale file cannot be re-read as good. #404's schema is a DECLARATION the model
consumes. The declaration is generated FROM the record, which is why the
dependency runs one way and why the record can be specified now.

**Acceptance.** A QEMU run with the cycle counter disabled fails with a message
naming the counter. The bench's output is machine-readable in whatever form
phase-357 W1 settles on.

**First half DONE 2026-08-16.** `just qemu test-wcet` exits 1 on QEMU and emits
ZERO rows matching `min=0 max=0 avg=0` (was 13, one per benchmark). The message
states why a zero is dangerous — it is indistinguishable from "this operation is
free", and zero is the most optimistic WCET there is, so consuming it always
errs toward "schedulable" — rather than merely reporting that the counter is
dead, which is what it did before while printing the zeros anyway.

That the recipe now FAILS on QEMU is the point: QEMU does not implement DWT
cycle counting, so this bench cannot measure there. It is `[group("debug")]`
and no CI lane runs it.

**Second half DONE 2026-08-16.** The producer is split across a binary and a
script, because the binary cannot be the whole answer: `wcet-cycles-qemu` is
`no_std` on Cortex-M with semihosting stdout as its only output channel, so it
cannot open a file. It prints each number twice instead — prose for a human, an
`NROS_WCET_V1` TSV record for a tool — and `scripts/bench/wcet-log-to-json.py`
turns the records into a `nros.wcet.measurements/1` artifact
(`just qemu wcet-artifact`). Direction items 1 and 3 are both covered: per
measurement `min`/`max`/`mean`/iterations and identity, plus `counter_valid`,
`cpu`, `profile` and `commit` (the last two baked by `build.rs`).

The split is also what made the work testable here. Two absences stay absences:
a log with no measurements produces NO file rather than an empty one, and a
missing `clock_hz` sets `convertible_to_time: false` rather than letting a
consumer assume a plausible rate — inventing one is the manufactured-WCET
failure #404 exists to prevent, one layer earlier.

**What is NOT verified, stated rather than implied.** The emitter has never been
observed emitting. QEMU refuses before the first marker line (the run produced 0
of them) and this tree has no hardware lane, so no run anywhere can currently
produce an artifact. The parser is tested — 8 self-test cases, run on every
conversion, including prose containing `min=0 max=0 avg=0` NOT parsing as data —
and the emitter is drift-checked against its real format string rather than
against a hand-written fixture, which is the mirror-drift class this tree keeps
hitting. Neither is a substitute for a run on a part with a live DWT.

## W3 — Native sched dims verified only on the FALLBACK arm (#260)

`core-pin` and `sporadic budget` are e2e-verified only where the kernel REJECTS
the request and the runtime falls back. No fixture exercises the accept path, so
the code that runs when the kernel says yes has never been observed working.

~~The obstacle is privilege: accepting these dims needs capabilities a normal
test host does not have, which is what
[phase-162](phase-162-rt-scheduling-harness.md) exists to stand up. This work
item is therefore **blocked on phase-162**.~~

**Struck 2026-08-17 — I wrote a blocker this work does not have.** #260's own
text says the sporadic (W5.9b), EDF (W5.5) and preempt-threshold (W5.10) accept
arms were already kernel-accepted, and the posix core-pin accept arm exists
precisely BECAUSE `sched_setaffinity` needs no privilege. What the four
remaining arms need is an image with more than one CPU — `CONFIG_SMP` /
`configUSE_CORE_AFFINITY` / `TX_THREAD_SMP`. That is a fixture-configuration
question. phase-162 is not in the way.

The paragraph also overstated the gap: it said `core-pin` *and* `sporadic
budget` are fallback-only. Sporadic is kernel-accepted, and the run below
proves it.

### DONE — the arm is declared and asserted, not merely tolerated

`Shape::AcceptOrFallback` asserted "accept marker OR fallback note", so a cell
passed identically on either arm. Each two-mode cell now declares the arm its
image is known to take — `AcceptOrFallback { expect: Arm }` — and a mismatch in
either direction fails. A capability silently LOST is caught; a capability
silently GAINED is caught too, which is the state #260 wants reached
deliberately rather than by accident.

Every cell also prints its arm. 12/12 cells, no skips:

```
sched-dim arm: [zephyr rust CorePin]              FALLBACK
sched-dim arm: [nuttx rust CorePin]               FALLBACK
sched-dim arm: [threadx-linux rust CorePin]       FALLBACK
sched-dim arm: [freertos cpp CorePin]             FALLBACK
sched-dim arm: [posix rust CorePin]               ACCEPT
sched-dim arm: [zephyr {rust,cpp,c} EdfDeadline]  ACCEPT
sched-dim arm: [nuttx cpp SporadicBudget]         ACCEPT
sched-dim arm: [nuttx rust TierPriority]          2/2 tiers ACCEPT
sched-dim arm: [threadx-linux rust PreemptThreshold] ACCEPT
sched-dim arm: [threadx-linux rust TimeSlice]     ACCEPT
```

**The only fallback arms in the tree are the four RTOS core-pins** — measured,
not re-derived from `#ifdef`s. `SporadicBudget = Accept` was the one
non-obvious declaration and was verified by building the fixture and running
it, not taken from #260's prose; it is also the cell that gained most, having
previously tolerated a regression to the fallback while #260 recorded the dim
as covered.

### What asking "so is the accept path exercised?" turned up

Chasing the full acceptance produced a finding bigger than the acceptance.
The four RTOS accept arms are not merely unrun — **they are not compiled**.
Each sits behind a preprocessor gate (`CONFIG_SCHED_CPU_MASK_PIN_ONLY`,
NuttX `CONFIG_SMP`, `TX_THREAD_SMP`, `configUSE_CORE_AFFINITY`) and **no
config in the tree sets any of them**, so every body is deleted by the
preprocessor in every image. #260 called them "COMPILE-VERIFIED ONLY (against
headers)"; they are not type-checked at all.

Acting on that immediately found a real defect —
[issue 0655](../issues/0655-zephyr-core-pin-cannot-succeed-on-running-thread.md):
the Zephyr arm pins `k_current_get()`, and Zephyr's `cpu_mask_mod` returns
`-EINVAL` for a RUNNING thread, so that arm could never have succeeded even on
a correct SMP image. It was also gated on a knob strictly narrower than the one
`k_thread_cpu_pin` needs. Correcting the gate to `CONFIG_SCHED_CPU_MASK` (which
does NOT require SMP) and enabling it on the existing uniprocessor fixture
compiled the call for the first time and produced a real `rc=-22` where the
never-compiled `#else` had been inventing `-88`. `realtime_tiers_e2e` still
passes and the EDF cell still reports ACCEPT.

That reorders the remaining work. The cheap half was never the fixture:

1. **Make each arm compile somewhere** — no SMP required, and it catches the
   API-misuse class #260 is actually worried about. Done for Zephyr; the other
   three still have never-compiled arms.
2. **Make one arm run and be observed accepting** — the SMP fixture, which now
   needs a REAL SMP board: Zephyr's `native_sim` cannot do it (the POSIX arch
   has no SMP support), so #260's "cheapest candidate" is not viable. The
   viable targets are `qemu_cortex_a53_smp` / `qemu_riscv64_smp`, i.e. a board
   bring-up, not a conf tweak.

**Acceptance (now).** The suite reports which arm each sched dim took. **MET** —
and exceeded: the arm is asserted, not only reported.
**Acceptance (full).** One SMP fixture flips a core-pin cell to the ACCEPT arm.
Unblocked but re-costed: a new SMP board, per above. Do not close #260 on the
partial, and note #0655 must be fixed first — otherwise the SMP fixture would
be built to exercise a call that cannot succeed.

---

## Deliberately not doing

* **Not adding new test cells.** RFC-0051 is explicit that new runtime tests
  join `matrix::CELLS` / `interop::CELLS` rather than becoming hand-coordinated
  files. Nothing here needs a new cell; W3 needs an existing one to tell the
  truth about which arm it took.
* **Not chasing flakes.** Full-sweep QEMU lanes flake under load (287-W7), and a
  solo red can be a stale-build artifact (issue 0268). That is a known,
  documented confounder, not this phase's subject.
