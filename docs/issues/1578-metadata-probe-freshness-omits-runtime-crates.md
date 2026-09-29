---
id: 1578
title: "`nros sync` judges a metadata sidecar current from the COMPONENT's sources alone, so a nano-ros change to what the probe reports is invisible — a stale `in_place` is reused"
status: open
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
