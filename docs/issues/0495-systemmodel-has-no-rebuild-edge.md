---
id: 495
title: "`rebuilds_on_model_touch` fails: touching the resolved model no longer forces a re-check"
status: open
type: bug
area: testing
related: [issue-0490, phase-330, phase-340]
---

## Symptom

`native_main_macro_misuse::rebuilds_on_model_touch` fails:

```
expected demo_entry to be re-checked after the model touch
stderr: Finished `dev` profile [optimized + debuginfo] target(s) in 0.04s
```

Cargo short-circuits entirely — 0.04 s, no `Checking demo_entry`.

## What is NOT the cause (checked, so nobody re-checks it)

**The rebuild edge exists.** `packages/cli/nros-build/src/lib.rs:242` emits

```rust
println!("cargo:rerun-if-changed={}", model_path.display());
```

alongside edges on the launch file (196), the `system.toml` dep (199) and the
plan path (243). A first pass looking only at `**/build.rs` missed it, because
the emitter lives in a library those build scripts call — worth recording, since
the obvious grep gives the wrong answer.

## Probable trigger, unproven

Issue 0490 (fixed 2026-08-10) removed a `rerun-if-changed` pointing at a path
that has not existed since phase-321. Cargo treats a missing input as
**permanently dirty**, and `nros-rmw-cffi` sits under every image — so before
that fix, ANY `cargo check` rebuilt the whole chain, and "was it re-checked?"
was trivially true no matter what the model did.

So this test may have been passing for the wrong reason for as long as 0490
existed, and 0490 unmasked it. That is a hypothesis: it matches the timing
(first observed in the sweep immediately after 0490 landed) and the mechanism,
but it has not been confirmed by reverting 0490 and re-running.

## The narrowed question

The test rewrites the model with **byte-identical content** after sleeping
1100 ms to clear cargo's fingerprint mtime resolution:

```rust
let body = fs::read_to_string(&model_yaml)?;
fs::write(&model_yaml, &body)?;      // same bytes, new mtime
```

So it depends on an **mtime-only** change firing `rerun-if-changed`. Two
candidates, and they want different fixes:

1. **Path mismatch** — the `model_yaml` the test touches is not the path the
   build script registered (the test passes a `model_home` tempdir; the script
   computes its own via `resolve_model_path`). Then the edge is fine and the
   TEST is wrong.
2. **mtime-only is genuinely not enough** for this edge as registered. Then the
   test is right and something upstream of it is wrong.

Establish which before changing either — an mtime-only touch that silently fails
to invalidate would matter well beyond this test.

## Reproduce

```sh
source ./activate.sh
cargo nextest run -p nros-tests --test native_main_macro_misuse -E 'test(=rebuilds_on_model_touch)'
```
