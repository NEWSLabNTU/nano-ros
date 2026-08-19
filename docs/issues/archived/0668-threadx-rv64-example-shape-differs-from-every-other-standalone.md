---
id: 668
title: "ThreadX-RV64 is the only standalone example that owns two entry points, so it is the only one where the panic handler has a placement question"
status: resolved
type: tech-debt
area: build/examples
related: [issue-0666, issue-0618]
---

## Symptom

Every standalone Rust example in the tree has ONE entry point. These six have two:

```
examples/qemu-riscv64-threadx/rust/<ex>/
    src/main.rs      nros::main!()                          → the bin (cargo)
    src/app_main.rs  <board>::app_main!(crate::register)     → the .a  (CMake)
                     nros::panic_to_platform!()              ← the extra line
```

Compare the shapes it should match:

| family | entry | artifacts |
| --- | --- | --- |
| freertos / nuttx / arm-baremetal | `main.rs`'s `nros::main!()` | 1 (bin) |
| zephyr | `app_main.rs`'s `nros::zephyr_component_main!()`, no `main.rs` | 1 (`.a`) |
| **threadx-rv64** | **both of the above, in one crate** | **2** |

## Why it matters for the panic handler specifically

This is the ONLY example family where "which file gets the `#[panic_handler]`?"
has a non-obvious answer, and phase-366 had to grow machinery for it:

- `main!()` reads `[lib] crate-type` and suppresses its emit when the package
  produces a `staticlib`, because the lib owns the item for BOTH artifacts (the
  bin inherits it through the rlib). That derivation exists for these six.
- They are also the only images still carrying a hand-written
  `nros::panic_to_platform!()` after M2 migrated the other 15.
- An earlier plan had them declare `panic = "own"`, which would have overloaded
  `own` — it means "I bring my own provider", not "my provider is in my other
  artifact" — and cost the design the deliberate-vs-forgot distinction that
  `own` exists to create. Rejected, recorded in RFC-0077.

None of that machinery is wrong. It is all in service of one family's shape.

## Root cause is #0666, this is its consequence

The two entry points exist because the two RMWs are built by different build
systems: zenoh via cargo (bin), cyclonedds via CMake (staticlib + C `startup.c`
calling `app_main`). That divergence is issue 0666. THIS issue is the part that
leaks into example source and into the panic design, and it is worth tracking
separately because it can be fixed by aligning the example shape even if the
build-path question in 0666 is answered differently.

## What "aligned" would look like

Either single-entry shape is fine; what matters is that it is one.

- **Zephyr's shape** — no `main.rs`, `crate-type = ["staticlib", "rlib"]`, entry
  is the lib-side macro. Both RMWs would build through CMake. Costs the pure
  cargo build, which is the faster inner loop and the one contributors use.
- **The freertos/nuttx shape** — no `app_main.rs`, `crate-type = ["rlib"]`, entry
  is `main.rs`. Requires cyclonedds to be reachable from a cargo build on this
  target, which is 0666's cargo-only direction.

Once it is one entry point, `main!()`'s `crate-type` derivation stops being
load-bearing for examples (it stays correct for genuine dual-artifact crates),
and these six lose their last hand-written panic line — the entry macro emits it
like everywhere else.

## Not in scope

The `#[global_allocator]` sits in the same place and has the same shape problem.
It is not included here because 0616/0594 already own the allocator's per-image
story and the two should not be conflated; the fix for this issue should just not
make that one worse.

## Resolved 2026-08-19 — phase-369, and one prediction was wrong

One entry. `src/main.rs` and the `[[bin]]` section are gone from all six;
`crate-type` stays `["staticlib", "rlib"]` and the lib-side `app_main!` is the
only entry — the Zephyr shape this issue named.

The hand-written panic line is gone too, but **not for the reason predicted
here.** This issue said:

> Once it is one entry point ... these six lose their last hand-written panic
> line — the entry macro emits it like everywhere else.

That holds for `nros::main!`, which does emit one. It does not hold for the
entry that SURVIVES: `app_main!` emitted only the extern function. So after the
`main.rs` deletion the hand-written `nros::panic_to_platform!()` was not
redundant — it was THE provider, and deleting it would have left the leaf
staticlib, a FINAL artifact, with no handler and reproduced 0688/0692's
`#[panic_handler] function required`.

Caught by reading the macro before running anything. The phase wave that would
have deleted the line (W5) was closed as not-applicable, and W6 delivered the
same end state properly: `app_main!` now TAKES the policy —
`panic = platform | own`, defaulting to platform, mirroring `nros::main!(panic = …)`.

The `own` arm is the load-bearing part. Emitting unconditionally would have made
the policy the library's, which is issue 0618's complaint and what phase-366
spent a phase undoing. RFC-0077 puts policy with the IMAGE; the image now names
it or takes the default.

### Verified

12/12 images carry exactly one `rust_begin_unwind`; 0 hand-written
`panic_to_platform` lines remain. The count is bound LOCAL (`t`) rather than
GLOBAL in the final ELF — 0692's `t`/`T` distinction is about ARCHIVES, and a
linked image legitimately localises the symbol. One provider, no duplicates,
which is the property that matters.

### Out of scope, as filed

`#[global_allocator]` has the same shape in the same files and is still
0616/0594's. This phase did not make it worse and did not try to fix it.
