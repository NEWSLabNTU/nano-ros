---
id: 851
title: "`zpico-alloc` uses `fetch_add` on riscv32imc, which has no atomic CAS —
  the esp32 fixture lane cannot build"
status: resolved
resolved_in: "issue-0851 (this filing)"
type: bug
area: memory
related: [phase-391, issue-0840]
---

## Symptom

`just build-test-fixtures lane=tier2` dies in the esp32 stage:

```
error[E0599]: no method named `fetch_add` found for struct `Atomic<T>`
   --> packages/rmw/zenoh/zpico-alloc/src/lib.rs:395:32
    |
395 |             self.foreign_frees.fetch_add(1, Ordering::Relaxed);
```

The ESP32-C3 fixture target is `riscv32imc-unknown-none-elf`. `imc` has no `A`
extension, so there is no atomic compare-and-swap and `fetch_add` does not
exist. `foreign_frees` is a diagnostic counter (`foreign_frees()` is its only
reader), so the portable form is a plain load + store — which is what the
SIBLING increment in `realloc` already does:

```rust
let n = self.foreign_frees.load(Ordering::Relaxed);
self.foreign_frees.store(n + 1, Ordering::Relaxed);
```

That site was written correctly; this one was not. Fixed by making them match.

## It was hidden behind a resolution failure

Before this, the leaf failed EARLIER, at dependency resolution: a gitignored
leaf lock pinned `rlsf 0.2.2` while `zpico-alloc` requires `^0.2.3`. I cleared
that with `just lock-update rlsf 0.2.3 …`, reported it as "stale lock residue,
nothing to file" — and the build then got far enough to reveal this compile
error. **The lock conflict was masking a real defect**, and the "nothing to
file" verdict was wrong. Same shape as issue 0845's two layers and the
`DdsSrvType` red: the first failure stops the build before the second is
reachable, so clearing it is not a no-op, it is how you find the next one.

## Open question: why only ONE of six sites

`used_bytes` is the same type (`core::sync::atomic::AtomicUsize`, imported at
line 61) and carries five more CAS calls:

```
258  used_bytes.fetch_add     277  used_bytes.fetch_sub
318  used_bytes.fetch_add     500  used_bytes.fetch_add
507  used_bytes.fetch_sub
```

None of them is cfg-gated, yet rustc reported `1 previous error`. **Answered by
building it:** with line 395 fixed,

    cd examples/qemu-esp32-baremetal/rust/talker
    cargo build --target riscv32imc-unknown-none-elf
    -> Compiling zpico-alloc … Finished

compiles clean. So the five `used_bytes` calls do NOT fail on this target and
the fix is complete, not one of six. Why they differ from `foreign_frees` is
still unexplained — but the question that mattered (is this fix whole?) is
settled by the build rather than by the reasoning that raised it.

Note also the error names a generic `Atomic<T>`, not `AtomicUsize`. That does
not match the declaration at line 132 and is unexplained; it may indicate a
shim type substituted for this target, which would change the analysis.

## Fix

Line 395 now uses the load+store idiom its sibling already used. Verified by
building the esp32 leaf for `riscv32imc-unknown-none-elf`: `zpico-alloc`
compiles and the leaf finishes.
