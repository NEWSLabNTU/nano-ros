---
id: 115
title: "rustc / ld crashes under heavy fixture-build load are caused by unstable host RAM (not a nano-ros bug)"
status: wontfix
type: bug
area: build
related: []
---

## Summary

`just build-test-fixtures` (and therefore `just ci`'s `test-all` / `cyclonedds-ci`)
intermittently fails on one dev host because **rustc crashes** mid-compile — `SIGSEGV`,
`general protection fault`, or `the compiler unexpectedly panicked` — on a *different*
crate each run (paste, toml, nros-macros, nros, nros-cpp). It looked like a non-deterministic
rustc bug. It is **not**. The root cause is **failing / unstable RAM on the host**, and the
crashes are the visible tip of silent memory corruption that affects *all* workloads on the box.

## Root-cause evidence (host kernel log, `journalctl -k`)

Over ~2 months (May 06 → Jun 29) the kernel logged faults across **many unrelated binaries**,
spread across CPU cores 2–7:

```
 31 × libLLVM.so        (rustc's LLVM codegen)
 27 × librustc_driver.so
  2 × python3.10 / 3.12
  2 × libtorch_cpu.so   (unrelated PyTorch workload)
  1 × x86_64-linux-gnu-ld.bfd   (the GNU linker)
  1 × libgcc_s.so.1
  1 × libc.so.6
```

Fault types: `segfault`, `general protection fault`, **`trap invalid opcode`**.

Why this is hardware, not a compiler bug:

- A fault **inside `libc.so.6`** — the most-exercised library on the system — means the libc
  **code page in RAM was corrupted**. libc does not have bugs that segfault.
- `libLLVM.so` / `ld.bfd` are **read-only shared pages**: one physical copy mapped into every
  process. A single bit-flip in that page makes every consumer fetch a garbage instruction →
  `invalid opcode`. Exactly the observed pattern.
- It hits **unrelated workloads** (Rust builds, PyTorch, python, the GNU linker) — no compiler
  bug crashes PyTorch.

Host: **AMD Ryzen Threadripper 2950X** (Zen+), **non-ECC** (no EDAC instances exposed) → bit-flips
go undetected and silently corrupt whatever is loaded. rustc crashes most because it is the
heaviest memory user under the all-32-thread fixture build, but the box corrupts everything.

## Ruled out

- **Not OOM** — 94 GiB RAM, ~89 GiB free during the build; no swap pressure; no kernel OOM kill.
- **Not sccache** — not installed on this host.
- **Not rustc's parallel front-end** — not enabled (`-Z threads` absent).
- **Not nano-ros code / a specific crate** — the crash hops between unrelated crates and even
  non-Rust binaries; 24 concurrent *fresh-target* `nros-macros` builds produced **zero** crashes
  (lower sustained memory pressure than the full fixture build).

## Why a retry wrapper was the wrong fix

A `RUSTC_WRAPPER` retry shim (`scripts/build/rustc-retry.sh`) was prototyped and then **reverted**.
On corrupting RAM it is actively harmful: **most bit-flips during a compile do not crash rustc —
they produce a subtly wrong `.o`/`.rlib` that links and runs, wrong.** The crashes are only the
visible subset. A retry that lets the build report "success" gives false confidence that artifacts
built on this box are trustworthy, when they may be silently corrupt. It also left partial/corrupt
intermediate objects (observed downstream as `rust-lld: relocation R_X86_64_GOTPCREL out of range`
from a corrupt `.rcgu.o`). No software change can make a host with bad RAM produce correct binaries.

## Resolution (host remediation — outside this repo)

`wontfix` in nano-ros: this is a host-hardware fault, not a code defect. To fix the host:

1. Run **memtest86+** from USB overnight — expected to show errors.
2. BIOS: **disable XMP/DOCP**, set DRAM to JEDEC (2933 → 2666), loosen timings; on Threadripper
   optionally bump SoC/DRAM voltage. This alone resolves most TR2950X memory instability.
3. **Reseat DIMMs; test one stick at a time** to isolate the failing module.
4. Verify cooling (2950X ≈ 180 W under all-core load).
5. Until fixed, **do not trust binaries built on this host** — use a known-good machine / CI runner
   for any release artifact.

Host-specific build notes also live in agent memory `box-fixture-sizes-probe-sigsegv`.
