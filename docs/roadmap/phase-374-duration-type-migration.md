# Phase 374: `ros-launch-manifest` v0.1.9 — timing fields carry their unit

**Status: not started.** Measured, scoped, and smaller than it looks.

**Upstream:** `ros-launch-manifest` v0.1.9, and its
`play_launch` phase-63 campaign (`docs/roadmap/phase-63-duration-type-campaign.md`
in that repo). Written from this side by an agent working upstream — the
numbers below come from bumping the pins here and reading the compiler, not
from estimating.

## What changed upstream, and why

`budget_us: 8` when the author meant 8 ms is off by three orders of magnitude,
it type-checks, and it flows into a scheduling parameter — on the Linux side,
into a reservation the kernel admits or rejects. Nothing in the schema could
catch it: both are valid integers.

v0.1.9 moves the unit from the field *name* into the *value*:

```yaml
budget: 8ms          # was budget_us: 8000
max_latency: 12ms    # was max_latency_ms: 12
```

Fourteen fields were renamed and retyped from bare `f64`/`u64` to a `Duration`
carrying nanoseconds.

## What this costs us: 11 sites in one file

Measured by bumping all seven pins to v0.1.9 and running `cargo check`:

| crate | errors |
|---|---|
| `nros-orchestration-ir` | **11**, all in `src/lib.rs` |
| `nros-cli-core` | **0** |
| `nros-macros`, `nros-tests` | the same 11, inherited transitively |

Every site is in two adjacent functions, `rtos_spec_from_model` and
`tier_from_model` (`lib.rs:627-665`) — the one conversion from the resolved
model into the IR that the proc-macro and `codegen-system` consume. They read
`TierPlatformSpec` and `TierDef` platform fields:

```rust
time_slice_us: spec.time_slice_us,   // -> spec.time_slice.map(|d| d.as_micros())
deadline_us:   spec.deadline_us,     // -> spec.deadline.map(|d| d.as_micros())
budget_us:     spec.budget_us,
period_us:     spec.period_us,
spin_period_us: t.spin_period_us,
```

Our own `TierRtosSpec` / `TierDef` keep their `_us` names and `u64` types —
nothing about the IR or the generated runtime needs to change. This is a
conversion-site edit, not a schema migration on our side.

**Do not trust grep here.** `grep -rn` for the fourteen field names reports
**919** hits in `packages/`, and 209 even after narrowing to files that import
a manifest crate. Almost all are our own RTOS types, which legitimately keep
microsecond-suffixed names. The compiler's 11 is the real number; upstream saw
the same ratio three times (217 grep hits → 7 real, in `play_launch`).

## Our data does not change

Every deprecated spelling still parses at v0.1.9 and means exactly what it
meant, so no contract or platform file has to be touched to move to the tag.
Only the canonical spelling is *written*, so re-emitting a file migrates it.

A new upstream lint, `deprecated-unit-suffix`, reports old spellings at **info**
severity — a nudge, not a failure. It skips endpoint and topic names, so a topic
called `.../debug/processing_time_ms` is not flagged.

One asymmetry worth knowing, because it is deliberate and tested upstream: a
bare number under a *new* name is refused by the contract reader (which can see
which spelling a document used) but read in the legacy unit through serde
(where `alias` cannot report which name matched). Platform files go through
serde, so `budget: 8000` there is read as 8000 µs rather than rejected.

## The pin is in four manifests

```
packages/cli/nros-cli-core/Cargo.toml
packages/core/nros-orchestration-ir/Cargo.toml
packages/core/nros-macros/Cargo.toml
packages/testing/nros-tests/Cargo.toml
```

Seven `tag = "v0.1.8"` lines across them, currently consistent. **They must
move together.** Naming one tag is what makes cargo resolve a single instance;
two revisions become two same-named packages from different sources, and
`TierDef` becomes two incompatible types. Bumping only `nros-cli-core` was the
first thing tried while measuring this, and the lockfile silently stayed at
v0.1.8 — the build succeeded and proved nothing.

Upstream hit the identical trap: `play_launch`'s `tests/` crate sat on a stale
pin for five tags because it drives a subprocess and no types cross the
boundary, so nothing failed to compile. The symptom is not an error, it is
silence.

## We also vendor play_launch

`packages/cli/third-party/play_launch` is a submodule, and play_launch is now
on v0.1.9. Updating that submodule without bumping our pins produces exactly
the two-revision split described above. **Bump the pins in the same change as
the submodule, or not at all.**

## Plan

1. Bump the seven pins across the four manifests; commit `Cargo.lock`.
2. Fix the 11 sites in `nros-orchestration-ir/src/lib.rs` — mechanical:
   `spec.<field>` → `spec.<renamed>.map(|d| d.as_micros())`, and
   `.as_micros()` where the field is not optional.
3. Verify that *no value moved*. This is the acceptance rule upstream used and
   it is the one that matters: every changed line should differ only in
   spelling and accessor. Any diff hunk where a **number** changes is a defect
   to explain, not a rename to skim.
4. Optionally migrate our own contract/platform files to the new spelling.
   Not required — both spellings parse — but it silences the lint and every
   file written in the old form makes the eventual sunset costlier.

## Risk

The mechanical risk is real and worth naming: the edit converts microsecond
integers into `Duration` by hand eleven times, and using the millisecond
constructor where microseconds were meant is *precisely the thousandfold error
this whole change exists to remove*. Upstream guarded that with an oracle —
`examples/rt_av_demo`, whose nodes burn a declared number of milliseconds, so a
slip anywhere in the chain reads 8000 ms instead of 8.06. We have no equivalent
oracle for the RTOS path; the mitigation here is step 3, plus a diff review that
treats a changed digit as a bug.

## Sunset

Upstream will eventually remove the deprecated names, gated on the contract
`version:` field. That removal is blocked on *this* phase and on the Autoware
contract corpus (already migrated). Nothing forces a schedule — v0.1.8 keeps
working until we choose to move.
