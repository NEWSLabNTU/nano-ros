---
id: 666
title: "ThreadX-RV64 builds one example two ways — cargo for zenoh, CMake for CycloneDDS — and the RMW is what picks the build system"
status: open
type: tech-debt
area: build/examples
related: [issue-0205, issue-0651]
---

## Symptom

`examples/qemu-riscv64-threadx/rust/<example>` is the only Rust example leaf in
the tree that is built by two different build systems, and which one you get is
decided by the RMW:

| backend | built by | artifact | entry |
| --- | --- | --- | --- |
| zenoh | cargo | bin | `src/main.rs`'s `nros::main!()` |
| cyclonedds | CMake + Corrosion | `staticlib` | `src/app_main.rs`'s board `app_main!()` |

So one directory carries `Cargo.toml` AND `CMakeLists.txt`, `crate-type =
["staticlib", "rlib"]`, and two entry points that must not both be active. Every
other Rust family has exactly one build system and one artifact.

## Why it is this way (not an accident of laziness)

CycloneDDS is a C++ CMake library with generated IDL descriptors. On an embedded
target those must be cross-compiled by CMake with the RTOS toolchain, and cargo
cannot drive that. Zenoh needs none of it — zenoh-pico builds as a C shim inside
the cargo graph — so the same example also builds as a plain cargo bin.

The reasoning is sound per-backend. The result is not: **the transport backend
decides the build system, the artifact kind, and where the entry point lives.**

## Why it matters beyond tidiness

nano-ros's promise is that the RMW is a swappable backend behind a stable ABI
(RFC-0054, RFC-0071). This leaf contradicts that at the build layer, and the
contradiction leaks into the source:

- **It produced an RMW-named API.** The board's C-ABI entry macro was
  `cyclonedds_app_main!()` until phase-366 W7 renamed it to `app_main!()`. Its
  body is `run_app_thread($register)` — nothing in it is CycloneDDS. The name
  encoded "the backend that happens to need CMake", so a second backend needing
  the CMake path would have had to misuse a `cyclonedds_`-named macro or clone
  it. Renaming fixed the symptom; the divergence that produced it is this issue.
- **It is the one family where lang-item placement is not obvious.** Two final
  artifacts from one crate means the `#[panic_handler]` must live in the lib
  (the bin inherits it through the rlib) rather than beside `main!()`. That made
  the six ThreadX examples the special case in phase-366's migration, and the
  rule "the entry macro of a final artifact emits that artifact's handler" only
  reads cleanly because it happens to cover both.
- **It doubles the build surface for one leaf.** Two configure paths, two sets
  of stale-fixture behaviour, two ways for the same example to break — and the
  CMake half is currently red for an unrelated reason (see below), which is
  exactly the kind of thing that hides in a path only one backend exercises.

## Current state of the CMake half

Red as of 2026-08-18, and not from phase-366:

```
rust-lld: error: undefined symbol: stderr
>>> referenced by log.c
>>>               log.c.obj:(init_lock) in archive .../lib/libddsc.a
rust-lld: error: undefined symbol: stdout
>>> referenced by q_init.c ... and picolibc's posixio.c
```

CycloneDDS's own runtime reaches for libc stdio on a bare-metal target.
Reproduced with all local edits stashed, so it is the upstream tree. This target
built clean earlier the same day; the strongest suspect among the commits in
between is `a19e1fdfb` ("the riscv64 lane resolves its toolchain instead of
spelling Ubuntu's") — a different riscv64 toolchain means a different libc, and
picolibc does not provide `stdout`/`stderr` as linkable objects. NOT diagnosed
further; recorded here because it is a property of the path this issue is about.

## Directions

Candidates, not a plan. Each has a real cost and the choice is a maintainer's.

- **Make the CMake path the only path for this family.** One build system per
  leaf, `staticlib` only, no `main.rs`, entry via `app_main!()` — which is
  exactly the shape the Zephyr examples already have (they have no `main.rs` at
  all). Costs the zenoh variant its pure-cargo build, and cargo-only is the
  faster inner loop.
- **Make cargo the only path**, by teaching the CycloneDDS backend to build its
  C++ half from a build script the way zenoh-pico's shim does. Removes the CMake
  leaf entirely; the cost is a build script that must cross-compile a C++ library
  plus IDL codegen, which is what CMake is good at.
- **Split the leaf per build system.** Honest but collides with RFC-0066 /
  phase-331 — "a FEATURE is a node package, a CONFIGURATION is a fixture axis,
  never a new directory" — and RMW is the canonical configuration axis.
- **Accept it and name it.** Keep both, but state in `examples/README.md` that
  this leaf is dual-build, so the next person does not read `crate-type =
  ["staticlib", "rlib"]` as a mistake and delete half of it.

Whichever way it goes, the invariant worth keeping is the one the rename
restored: **no build-system fact may be spelled with an RMW name.**
