# Phase 378 — `ros-launch-manifest` v0.1.11: timing fields carry their unit

**Status (2026-08-24). W1–W4 landed; phase complete.** Measured before it was
planned, and far smaller than a grep suggested — the estimate held: 12 errors in
one file where a grep reported 687.

Landed as `4ad76cc59` (W2), `2d7bc591a` (W4 + the blocker below), and the W1
commit. **Those first two commits say `phase-374` in their subject and that is
wrong** — they inherited the number from `phase-374-duration-type-migration.md`,
which this doc supersedes and which is now deleted. Phase 374 is
`phase-374-test-suite-speedup.md`, unrelated. Recorded here rather than
rewritten, since the commits are on `main`.

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

**Landed** as `scripts/bump-manifest.sh` + `just bump-manifest <tag> [--dry-run]`.

The manifest list is **discovered**, not hardcoded to the four above, so a fifth
pin cannot drift in unnoticed — the exact way play_launch's own manifest sat four
tags behind. Discovery keys on a dependency KEY at line start
(`^ros-launch-manifest... =`): two manifests mention this crate only in prose and
must not be rewritten. Workspaces come from `cargo locate-project --workspace`,
never a path-prefix test, because `packages/cli` is a separate workspace inside
this repo (issue 0616).

Verified, each behaviour actually exercised rather than argued:

| behaviour | result |
| --- | --- |
| bogus tag | refuses, lists the real tags, changes nothing |
| `--dry-run` | reports the plan, changes nothing |
| real move (v0.1.11 -> v0.1.10) | 4 manifests + 2 locks move together, one revision each |
| round trip back | tree byte-identical to where it started |
| mid-run failure (unwritable dir) | restores every file; no half-bump |

The failure case had to be injected with an unwritable DIRECTORY: `sed -i` writes
a temp file and renames, so a read-only file does not stop it — a first attempt at
this test passed while proving nothing.

It also reports, without failing, any other tracked lock holding a different rlm
revision. `packages/cli/nros-launch-resolve` reaches rlm transitively through
play_launch's layer 2 and keeps its own lock deliberately, so it is out of scope
for a rewrite — but a silent omission there would let a "single revision" verdict
coexist with a second revision in the tree.

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

**Outcome: empty, and the doc's own advice is why.** "Count it with the compiler
before scoping" — the compiler counts ZERO. The predicted first site,
`PathContract { max_latency_ms: Some(5.0) }` at `model_ingest.rs:857`, still
compiles because `ros_launch_manifest_model::PathContract` **still declares**
`max_latency_ms: Option<f64>` at v0.1.11. The contract fields moved in the
`types` crate; the `model` crate's mirror of them did not. `cargo check
--all-targets` is clean in the `packages/cli` workspace. The 96 candidate sites
were 96 non-events.

### W4 — adopt the units in this repo's own data

**Correction, found by doing it: "both spellings parse" was FALSE here, so W4
was not optional and not readability.** It is true of rlm's parsers. It is not
true of ours. `system.toml` `[tiers.*]` has TWO parsers — the resolver's
`ros_launch_manifest_sched::TierPlatformSpec` (renamed, aliased) and this repo's
`nros_orchestration_ir::TierDef`, which parses the SAME block with
`#[serde(deny_unknown_fields)]` and knew only `_us`. The documented new spelling
was therefore accepted by one mirror and REJECTED by the other, so migrating the
data broke every file until `TierDef`/`TierRtosSpec` learned the new names as
aliases (delegating to rlm's own `compat::opt_micros`, so there is no second
duration parser here).

That is issue 0380's shape exactly — two mirrors of one block, the narrower one
silently defining what a user may write — and the comment already on
`TierRtosSpec` warns about this very pair. Nothing exercised that schema before:
other tests construct the structs directly or read keys off a `toml::Value`.
`tests/system_toml_tiers_parse.rs` now parses every tracked `system.toml`
through the real type.

The rest of the item stands as written: platform files and contracts shipped here can move to
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

  **Confirmed and closed.** Measured, not reasoned about: `deadline = "500ns"`
  gave `deadline_us = Some(0)` and `"1500ns"` gave `Some(1)`. A deadline of ZERO
  is a different statement from no deadline, arrived at silently — the very
  "a written value becomes a different value" class this upstream change exists
  to remove, so reintroducing it at the seam that adopts it would be perverse.
  `opt_micros_u64` now REFUSES anything that is not a whole number of
  microseconds (and anything negative, which `as_micros()` clamps to the same
  zero), naming the value and what it would have become. Test:
  `durations_finer_than_a_microsecond_are_refused_not_floored`.
- **Do not migrate this repo's own vocabulary.** The 687 grep hits are mostly
  nano-ros types. Renaming them is a much larger change that this upstream bump
  does not require and should not be bundled with.
