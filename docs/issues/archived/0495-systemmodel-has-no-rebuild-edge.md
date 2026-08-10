---
id: 495
title: "`rebuilds_on_model_touch` fails: touching the resolved model no longer forces a re-check"
status: resolved
resolved_in: phase-342
type: bug
area: testing
related: [issue-0490, issue-0501, issue-0414, phase-330, phase-340]
---

## Resolution (2026-08-10) — neither candidate; the artifact was never tracked

**The test was right and the edge was missing.** Both candidates below are
wrong, and so is the "probable trigger":

* Not a path mismatch. The test touches exactly the path the macro reads.
* Not "mtime-only is insufficient". Cargo tracks `include_bytes!` inputs fine.
* Not issue 0490 unmasking it. It reproduces cold, alone, with the shared
  target dir wiped.

The macro reads the model at `main_macro.rs:589` and **never registers it**.
`ensure_model` returns `(model_path, inputs)` where `inputs` is only
`[system.toml, launch file]` — deliberately, per a comment in its self-resolving
branch — so `tracked.extend(inputs)` never sees the artifact. The module header
states the invariant this violates outright: *"we emit `include_bytes!` for
every file the macro read"*. The model is read and is not among them.

`demo_entry` has no `build.rs` (entry crates deliberately carry none), so
`nros-build`'s `cargo:rerun-if-changed` — the edge the issue confirmed exists —
never runs for this fixture at all. That confirmation was true and irrelevant.

### The fix is asymmetric, and the asymmetry is the point

`ensure_model` now returns the model among the rebuild deps **only in the
build-produced branch**. The self-resolving branch must keep excluding it: that
branch WRITES the file, and depending on your own output is the perpetual-dirty
loop the `is_fresh` check right above it exists to prevent. Where the function
is a reader of someone else's artifact, tracking is correct; where it is the
writer, it is not. Only a writer can loop on its own output.

### Why it matters past the test

A build-produced model can change without its inputs changing — `nros sync` on a
newer CLI, a different resolver, an expert `MODEL` override. A consumer tracking
only the inputs then compiles against a model it cannot notice changing, which
is the museum-binary shape.

Verified: 5/5 warm ×3, cold, and serial. Tripwired — reverting just this hunk
fails `rebuilds_on_model_touch` alone, with the other four passing.

**Not the same bug as #501**, though they shared a file and a
`Finished in 0.0Ns` symptom; #501 asserted a link between them and that link is
retracted there. Each fix was reverted independently to prove the other did not
mask it.

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
