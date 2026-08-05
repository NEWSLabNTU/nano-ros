---
id: 423
title: Borrowed-view RUNTIME e2e (C/C++) was orphaned + bit-rotted; runtime coverage lost
status: open
type: tech-debt
area: testing
related: [rfc-0033, phase-329]
---

## Problem

The borrowed-view (RFC-0033, issue 0021) RUNTIME end-to-end proofs —
`tests/borrowed_c_e2e.sh` and `tests/borrowed_cpp_e2e.sh` — were **orphaned and
bit-rotted**. They compiled+linked a driver against the generated borrowed
sources and RAN it, asserting every borrowed view (C `nros/borrowed.h` helpers;
C++ `nros::Span`/`StringView`/`LeSpan`) points INTO the CDR buffer with correct
values — the only RUNTIME check of the borrowed path.

Two independent rots made them dead code masquerading as coverage (phase-329 W5
follow-up, 2026-08-05):

1. **No runner.** No justfile recipe, `just` lane, or CI workflow invoked either
   shell. Grep across `justfile`/`just/`/`scripts/`/`.github/` finds nothing.
   They had provided zero coverage for however long.
2. **They no longer build.** Two breakages accumulated:
   - The RFC-0042 D1 header collapse moved `<nros/platform.h>` to
     `nros-platform-api/include`; the shells' include sets never gained it, so
     the generated `#include <nros/platform.h>` fails outright.
   - The `nros_config_variant_sz_<hash>` config-variant GUARD was added after the
     shells were written. The generated borrowed `.c`/`.hpp` include
     `nros_config_generated.h`, which references the variant symbol `extern`; a
     standalone `cargo build -p nros-c` stubs the config (`EXECUTOR_SIZE probe
     returned 0` — the sizes/opaque class, cf. issues 0088/0245/0268) so the
     archive defines NO matching variant symbol → the driver link fails
     `undefined reference to nros_config_variant_sz_…`.

## Resolution taken

Deleted the two dead shells, their driver fixtures
(`fixtures/borrowed-{c,cpp}-e2e/`), and their rows in the phase-329 W5
negative-diagnostic registry (they described tests that neither ran nor built).
A work-in-progress build-stage recipe (`scripts/build/borrowed-e2e-fixture.sh`,
the intended E1-compliant replacement) got as far as the two rots above before
hitting the config-variant wall, and was removed rather than left half-built.

## What still covers borrowed, and the gap

- **EMIT / compile coverage survives**: `rosidl-codegen`'s `emit_c_borrowed_e2e`
  and `emit_cpp_borrowed_e2e` (`#[ignore]`) generate the borrowed sources and are
  reachable via `just run-ignored` (issue 0328). They do NOT run the driver, so
  the borrowed-VIEW-points-into-the-buffer RUNTIME assertion is currently
  **unguarded**.

## Root cause (sharpened 2026-08-05)

The blocker is NOT merely a missing include or a stale header — it is that **a
standalone `cargo build -p nros-c` produces an archive that is not meant to be
linked at all.** Traced through `nros-build-helpers/src/c.rs`:

- The `nros_config_variant_sz_<suffix>` anchor (a `__attribute__((weak))` C symbol,
  `c.rs:458-472`) is emitted ONLY when `suffix` is `Some` (`c.rs:455`), and the
  suffix is size-derived. When the `EXECUTOR_SIZE` probe returns 0 (`c.rs:127-135`)
  the suffix is absent, so **no variant symbol is compiled into `libnros_c.a`**.
- The probe returns 0 for a standalone `cargo build -p nros-c` because
  `EXECUTOR_SIZE` is measured from `nros`'s `__NROS_SIZE_*` export symbols, which
  only carry a real value when a downstream binary INSTANTIATES the executor. The
  build.rs warning says it outright: *"EXECUTOR_SIZE probe returned 0 … do not link
  the resulting rlib."*
- The `d021…` hash in `target/nros-c-generated/nros_config_generated.h` is left
  over from an earlier, properly-sized build; a fresh standalone build is the stub,
  so header (`extern nros_config_variant_sz_d021…`) and archive (no such symbol)
  disagree → `undefined reference`.

## Direction

Re-establish the runtime proof as a build-stage fixture, but the driver must link
against an `nros-c` archive built THROUGH A CONSUMER THAT INSTANTIATES THE EXECUTOR
(so the size probe is non-zero and the variant anchor is emitted), not a raw
`cargo build -p nros-c`. Two viable shapes:
  1. **C++ side is the easier win**: its proof already builds an FFI staticlib crate
     (`ffi_wrapper.rs`) that depends on nros-c and instantiates it — that build
     sizes the executor, so `libborrowed_cpp_e2e.a` should carry the matching
     variant. Rebuild the C++ proof around that archive + the header IT emits.
  2. **C side needs a sized carrier**: link the C driver against an archive from a
     cargo target that instantiates the executor (an example's, or a purpose-built
     `demo_entry`-style crate), not the bare umbrella archive.
This is the sizes/opaque integration (Phase 87/118/119 class), not a cleanup — it
is why the shells rotted silently. The driver logic to reuse lived in the deleted
`fixtures/borrowed-{c,cpp}-e2e/` (recoverable from git history at this change's
parent).
