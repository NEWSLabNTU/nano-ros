---
id: 1578
title: "`nros sync` judges a metadata sidecar current from the COMPONENT's sources alone, so a nano-ros change to what the probe reports is invisible — a stale `in_place` is reused"
status: resolved
resolved: 2026-09-29
resolved_in: "one freshness key, `probe_inputs_key`, at all four sites -- the component, the CLI, and the nano-ros crates a probe compiles (generated `probe-source-dirs.txt`, gated)"
type: bug
area: [tooling, build]
severity: medium
found: 2026-09-29
related: [1340, 1522, 1018, 1360, 0196, 0627]
---

## What

`orchestration/metadata_refresh.rs` decides whether to re-run the metadata
probe with:

```rust
let digest = source_digest(&decl.package_root)?;
if sidecar_is_fresh(&sidecar, &digest) { /* reuse */ }
```

`source_digest` is FNV-1a over the CLI's `CARGO_PKG_VERSION` plus every file
under **the component's own package**. `sidecar_is_fresh` is exactly "the
recorded `inputs_digest` equals that". Nothing from nano-ros's own crates
enters it.

But the probe's OUTPUT depends on them. It compiles the component against
`nros` / `nros-node` from this checkout and records what they report — since
phase-457 W5, the `in_place` flag of every declared subscription comes from
`nros-node`'s `DeclaredSubscriptionShape::in_place_capable()`. A change there
changes what the probe would say and leaves the digest untouched.

`CARGO_PKG_VERSION` does not cover it either: it is constant across every
development build of the CLI.

## Reproduced, 2026-09-29

`examples/native/rust/listener`, toggling that one bool in
`packages/core/nros-node/src/executor/declared_shape.rs`, `nros sync` each
time:

| step | bool | `sync: source metadata —` | sidecar `in_place` |
| --- | --- | --- | --- |
| 1 | `true` | 1 rebuilt | `true` |
| 2 | `false` | 1 rebuilt | `false` |
| 3 | `true` | **0 rebuilt, 1 already current** | **`false`** |

At step 3 the source says `true` and the sidecar says `false`, and the
descriptor states `registration_path = "unbounded"`. Deleting the (gitignored)
sidecar forced a re-probe and it read `true`.

**Step 2's re-probe is unexplained.** The package did not change between steps
1 and 2 either, so by the code above step 2 should also have read "already
current". Recorded rather than guessed at: the check is not a function of the
inputs that decide its answer, and it is not obviously deterministic in the
ones it does read.

## Why it matters

The observed direction was safe — a stale `false` states `unbounded` and the
build keeps a receive region the executor no longer claims. **The reverse is an
under-size**: any change that moves a shape from `true` to `false` leaves an
existing checkout's sidecar at `true`, the descriptor keeps pricing that
endpoint at no receive region, and the executor now claims one —
`NodeError::BufferTooSmall` at the first registration. That flip is exactly the
kind of change issue 1340's commit made in the other direction, and nothing in
`nros sync` would notice it.

This is issues 1018 / 1360 one tool over: a freshness key that names the
artifact's SOURCE but not the tool that interprets it. Those fixed the codegen
edge by adding the tool and then its EMITTED VERSION as freshness inputs.

## Fix direction (not decided)

Fold into the digest what the probe's output actually depends on. Two
candidates, both precedented here:

- **the nano-ros crates the probe compiles**, by the same generated-closure
  mechanism the CLI's own stamp uses (issue 0627's `cli-source-dirs.txt`)
  rather than a hand-written list — issue 0196's rule is that the probe watch
  what decides its answer;
- **an explicit probe-schema/semantics version** the harness emits and the
  digest includes, bumped whenever what a probe REPORTS changes — cheaper, but
  it relies on someone remembering to bump it, which is the failure mode the
  first option exists to remove.

Acceptance: step 3 above re-probes without deleting the sidecar.

## Resolution (2026-09-29)

**One key at every freshness site.** `probe_inputs_key(package_root, nano_ros)`
mixes three things, which are what a probe's report actually depends on:

1. the component's own sources (`source_digest`, the old key alone);
2. the CLI that wrote the harness (`NROS_CLI_SOURCE_STAMP`, baked by
   `build.rs` from the CLI's own sources — `CARGO_PKG_VERSION`, the only CLI
   input the old digest mixed, is constant across development builds);
3. the nano-ros crates the probe COMPILES, hashed from
   `packages/cli/probe-source-dirs.txt`.

It replaces THREE spellings: the plain digest (the Rust positive AND negative
caches), and `unprobeable_key` (digest + CLI stamp, the C/C++ negative cache
only). All four sites now read one function. `None` — no nano-ros path, or a
missing or empty list — is never fresh, which is `cli-source-dirs.txt`'s rule
for the CLI stamp: never a key over a smaller closure.

**The closure is generated, not hand-written.** `scripts/gen-probe-source-dirs.py`
records `cargo tree -p <pkg> -F <features> -e normal,build` for the roots the
two probe builders actually use (`nros` with `std,metadata-mode`,
`nros-platform-cffi` with `posix-c-port`, `nros-cpp` with `metadata-mode`,
`nros-c`). `cargo tree -p` and not `cargo metadata`: the latter unifies features
across the workspace and would have pulled every RMW backend in. 28 crates,
including `packages/rmw/metadata` — the recording backend that produces the
rows, which a hand-written list would most likely have missed.
`check-probe-source-dirs` (fast line) fails on drift and names the direction;
it runs its own negative controls on every invocation, and a mutation of either
the third-party filter or the drift direction was confirmed to fail them.

**Acceptance, re-run on the reproduction above** (`examples/native/rust/listener`,
toggling `DeclaredSubscriptionShape::BufferedRaw.in_place_capable()`):

| step | source | `nros sync` | sidecar `in_place` |
| --- | --- | --- | --- |
| 0 establish | `true` | 1 rebuilt | `true` |
| 1 flip | `false` | 1 rebuilt | `false` |
| 2 flip back | `true` | **1 rebuilt** (was `0 rebuilt`) | **`true`** (was stale `false`) |
| 3 no change | `true` | **0 rebuilt, 1 already current** | `true` |

Step 3 is the control that matters as much as step 2: a key that went stale on
EVERY sync would pass 0–2 too. The cache still hits when nothing changed.

**A correction to issue 0641's premise.** Its comment on `unprobeable_key`
called a stale POSITIVE sidecar "fine — a stale sidecar is caught by the coverage
gate". This issue was a stale positive sidecar that nothing caught, and whose
reverse direction under-sizes. The positive cache is held to the same standard
now; the comment that made the claim is gone with the function.

**Still unexplained:** step 2 of the ORIGINAL reproduction re-probed when, by
the old code, it should not have. The old key is gone, so this no longer
matters for correctness, and it is recorded rather than guessed at.

**Scope held:** `recompute_digest` (the phase-463 entity census) also calls
`source_digest`, and was deliberately left alone — it answers what the source
CREATES, which is a question about the component's own files.

