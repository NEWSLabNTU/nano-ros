# Phase 378 — `ros-launch-manifest` v0.1.11: timing fields carry their unit

**Status (2026-08-24). Not started. Measured, and far smaller than a grep
suggests.**

Every number below comes from bumping the pins in this repo and reading the
compiler, not from estimating. The headline: **12 errors in one file, inside a
30-line window.** A grep for the affected field names reports **687** hits.

**Supersedes** `phase-374-duration-type-migration.md`, which targeted v0.1.9,
predates three more tags, and collided with `phase-374-test-suite-speedup.md`.
Delete it when this lands.

**Upstream:** `ros-launch-manifest` v0.1.9–v0.1.11, driven by `play_launch`'s
phase-63 campaign (`docs/roadmap/phase-63-duration-type-campaign.md` in that
repo). Written from this side by an agent working upstream.

## What changed upstream

Time-valued fields carry their unit in the **value**, not in the name:

```yaml
budget_us: 8000        ->  budget: 8ms
max_latency_ms: 5.0    ->  max_latency: 5ms
rr_timeslice_us: 100000 -> rr_timeslice: 100ms
```

`budget_us: 8` when the author meant 8 ms is off by three orders of magnitude,
type-checks, and flows straight into a scheduling parameter. Fourteen fields
moved across v0.1.9–v0.1.11:

| crate | fields |
|---|---|
| `types` (contract) | `max_latency`, `max_response`, `max_transport`, `max_age`, `max_interval`, `jitter`, `lifespan`, `timeout` |
| `sched` (platform) | `deadline`, `budget`, `period`, `spin_period`, `time_slice`, `rr_timeslice` |

**Every old spelling still parses.** Each field carries `alias = "<old_name>"`
and a `compat::opt_micros`/`opt_millis` deserializer, so no YAML on disk breaks.
What changes is the **Rust field name and type** — `Option<u64>` /
`Option<f64>` become `Option<Duration>`.

So this is a source migration, not a data migration. Existing platform files
and contracts keep working untouched.

## The measurement

Pins bumped to v0.1.11, `cargo check` run, errors counted:

| | count |
|---|---|
| grep for the 14 field names across `packages/` | **687** |
| `cargo check -p nros-cli-core --all-targets` | **12** |
| files affected | **1** |
| line span | `nros-orchestration-ir/src/lib.rs:632–661` |

A **57× overstatement**, and the reason is worth internalising before planning
from a grep again: nearly every hit is one of *this repo's own* identically
named fields. `TierRtosSpec::time_slice_us`, `TierDef::period_us`,
`spin_period_us` are nano-ros's vocabulary and are **not** changing. Only the
places that *read from the manifest crate's types* are affected.

### The whole migration is one seam

Both failing functions are in `packages/core/nros-orchestration-ir/src/lib.rs`:

- `tier_rtos_from_model` (632–642)
- `tier_from_model` (657–661)

They are the single conversion from the resolved model into the IR that the
proc-macro and `codegen-system` consume — a fact that crate's own comment
already flags ("This is the ONE conversion from the resolved model into the IR
… so a field missing here is a scoped dim that silently never reaches the
runtime"). Every change is the same shape:

```rust
-  period_us: selected.and_then(|sp| sp.period_us).or(t.period_us),
+  period_us: selected
+      .and_then(|sp| sp.period.map(|d| d.as_micros()))
+      .or(t.period.map(|d| d.as_micros())),
```

nano-ros's own `TierDef`/`TierRtosSpec` keep their `_us` names and `u64` types.
Converting at this seam is deliberate and matches what upstream did at its own
boundaries: microseconds are what the RTOS bake speaks, so there is no reason
to thread a `Duration` into the IR.

### 12 is a floor, not a total

`nros-cli-core` never type-checks its own targets while
`nros-orchestration-ir` fails first. At least one further site is known:

```
packages/cli/nros-cli-core/src/orchestration/model_ingest.rs:857
    PathContract { max_latency_ms: Some(5.0), .. }
```

— a **manifest** type in a struct literal, in a test. A bounded grep over
`nros-cli-core/src` finds 96 candidate sites; on the evidence of wave 1, expect
the compiler to confirm a small fraction. Do not plan wave 2 from that 96.

## The finding that costs more than the migration

**Four manifests pin this crate, across two workspaces, and they must move
together:**

```
packages/cli/nros-cli-core/Cargo.toml        types, model, sched
packages/core/nros-orchestration-ir/Cargo.toml      model, sched
packages/core/nros-macros/Cargo.toml                model
packages/testing/nros-tests/Cargo.toml              model
```

Bumping only `nros-cli-core` — the obvious single edit — does not fail with a
missing field. It fails like this:

```
expected `TierDef`, found `ros_launch_manifest_model::TierDef`
  .../checkouts/ros-launch-manifest-<hash>/1a53088/sched/src/types.rs:23
  .../checkouts/ros-launch-manifest-<hash>/ce0b918/sched/src/types.rs:23
```

Two revisions of the same crate resolve as **two same-named, incompatible
types**, and the error points at a type mismatch rather than at the pin that
caused it. This was reproduced here, not theorised.

`play_launch` hit the same class of problem with three manifests and responded
with a `just bump-manifest <tag>` recipe that validates the tag on the remote
*before* editing anything, rewrites every manifest, refreshes every lockfile,
and then verifies each lock names exactly one revision. Its CLAUDE.md also
records which manifest drifts unnoticed and why — theirs sat four tags behind
because it uses the crate for one call, so nothing failed to compile.

**Recommendation: build the equivalent here first.** Four manifests across two
workspaces with no tool is a bigger standing hazard than the 12 field reads,
and it will recur at every future tag.

## Work items

### W1 — a bump tool

`just bump-manifest <tag>` (or the local equivalent): validate the tag exists
on the remote, rewrite all four manifests, refresh both lockfiles, then verify
each lock names exactly one revision and that it is the requested one. Refuse
rather than half-apply.

**Acceptance:** running it with a bogus tag changes nothing; running it with a
real one leaves `cargo tree` showing a single `ros-launch-manifest-*` revision.

### W2 — bump to v0.1.11 and fix the seam

All four manifests together, then the 12 reads in
`nros-orchestration-ir/src/lib.rs`. Mechanical: `.map(|d| d.as_micros())` at
each site.

**Acceptance:** `cargo check --all-targets` clean in both workspaces; no
`_us`/`_ms` field of the manifest crate read anywhere. nano-ros's own field
names are unchanged — a diff that renames `TierRtosSpec::time_slice_us` has
gone too far.

### W3 — the second wave

Whatever `nros-cli-core` surfaces once W2 unblocks it, starting with the
`PathContract` literal at `model_ingest.rs:857`. Count it with the compiler
before scoping.

**Acceptance:** full workspace green, tests included.

### W4 — adopt the units in this repo's own data

Optional and last. Both spellings parse, so this is readability, not
correctness: platform files and contracts shipped here can move to
`budget: 8ms`. Upstream's own acceptance criterion is the one to reuse — a
migration must show up as *"a unit suffix appearing, never a value moving"*.
It verified that by resolving the same launch under both spellings and diffing
the models; the only difference was the platform file's own `sha256`.

The deprecation lint added in v0.1.9 (`check/src/rules/deprecated_unit_suffix.rs`)
reports the old spellings, so this wave has a checklist rather than a grep.

## Risks

- **Half a bump is worse than none.** See the two-`TierDef` failure above. W1
  exists to make that unrepresentable.
- **`as_micros()` is lossy by construction.** `Duration` holds nanoseconds;
  the IR takes microseconds. That is the existing contract — the fields were
  already `_us` — but a contract written as `500ns` would now truncate to 0
  where before it could not be expressed at all. Worth one test.
- **Do not migrate this repo's own vocabulary.** The 687 grep hits are mostly
  nano-ros types. Renaming them is a much larger change that this upstream bump
  does not require and should not be bundled with.
