# Phase 356 — Test evidence: a sweep you can read afterwards, and numbers you can believe

**Status (2026-08-16). W2 DONE; W1 and W3 not started.** Three issues about
the same failure: a test run that reports something which cannot be checked, or
which is not what it appears to be.

* **W2 (#403)** — DONE, both halves. The bench no longer prints a table of zeros
  from a run whose cycle counter never counted: it refuses and exits FAILURE.
  And it now emits `NROS_WCET_V1` marker records beside the prose, which
  `scripts/bench/wcet-log-to-json.py` lifts into a `nros.wcet.measurements/1`
  artifact. That unblocks [phase-357](phase-357-wcet-as-declared-data.md) W1 —
  #404 has a real artifact to design a schema against instead of guessing.
* **W1 (#527)** and **W3 (#260)** — not started.

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

The obstacle is privilege: accepting these dims needs capabilities a normal test
host does not have, which is what
[phase-162](phase-162-rt-scheduling-harness.md) exists to stand up. This work
item is therefore **blocked on phase-162**, and is recorded here rather than
attempted.

What can be done now without privilege: make the fallback arm SAY it is the
fallback arm, so a future accept-path run is distinguishable from today's pass.
A test that passes identically under both is the reason this went unnoticed.

**Acceptance (now).** The suite reports which arm each sched dim took.
**Acceptance (full).** Blocked on phase-162; do not close #260 on the partial.

---

## Deliberately not doing

* **Not adding new test cells.** RFC-0051 is explicit that new runtime tests
  join `matrix::CELLS` / `interop::CELLS` rather than becoming hand-coordinated
  files. Nothing here needs a new cell; W3 needs an existing one to tell the
  truth about which arm it took.
* **Not chasing flakes.** Full-sweep QEMU lanes flake under load (287-W7), and a
  solo red can be a stale-build artifact (issue 0268). That is a known,
  documented confounder, not this phase's subject.
