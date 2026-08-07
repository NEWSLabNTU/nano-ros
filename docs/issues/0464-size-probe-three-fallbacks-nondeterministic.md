---
id: 464
title: "The size probe has three stacked fallbacks; losing a timing race silently substitutes stale constants into C storage sizes"
status: open   # layers 2+3 removed 2026-08-06; NuttX verification outstanding
type: bug
area: build
related: [phase-340, issue-0196, issue-0351]
---

## Symptom

Nothing fails. That is the problem.

`nros-c` / `nros-cpp` derive the opaque-storage macros in
`nros_config_generated.h` (`NROS_EXECUTOR_SIZE`, `NROS_PUBLISHER_SIZE`, …) from
Rust's `size_of::<T>()` by probing a compiled rlib. The probe has **three**
mechanisms, tried in order, each silently covering the previous one's failure:

| # | mechanism | determinism |
| --- | --- | --- |
| 1 | `find_dep_rlib_isolated` — nested cargo into its own target dir | deterministic |
| 2 | `find_dep_rlib_filesystem` — **polls the outer target dir**, `NROS_SIZES_PROBE_TIMEOUT_SECS` default 60 s | **a race** |
| 3 | `NUTTX_FALLBACK_SIZES` — committed constants in `nros-build-helpers/src/shared.rs` | **a stale literal** |

Layer 2 falls back to layer 1 with `cargo:warning=…`; layer 3 fires when the map
comes back empty. Cargo warnings from a build script are invisible in a normal
build, so all three transitions are effectively silent.

## Why it matters — the values are not equivalent

The committed layer-3 constant is **below the real size**:

```
NUTTX_FALLBACK_SIZES  EXECUTOR_SIZE = 79_296
measured (host)       EXECUTOR_SIZE = 89_392
```

These macros size the opaque byte arrays that C and C++ callers allocate for
Rust types. An under-sized `EXECUTOR_SIZE` is not a wrong number in a report; it
is a too-small buffer. The code comment at the injection site says so:

> …the consumer drops to the committed `NUTTX_FALLBACK_SIZES` — which silently
> rots below the real `size_of::<Executor>()` and trips the
> `EXECUTOR_OPAQUE_U64S too small` const assertion.

So today the const assertion is what catches it, in the consumer, at a distance
from the cause — and it catches it for **2 of 15** opaque macros. See "The
backstop covers 2 of 15 macros" below; for the other thirteen nothing catches it
at all.

## Root cause — two mechanisms each covering the other's gap

Layers 1 and 2 exist because neither is complete:

* Layer 2 needs the rlib to exist before the build script runs. Nothing orders
  it, hence the poll — so `nros-c` and `nros-cpp` each declare `nros` as a
  **build-dependency purely to force ordering**:

  ```toml
  [build-dependencies]
  # Phase 77.25: force nros to compile before this build.rs runs so the size
  # probe has a rlib to read.
  nros = { version = "0.5.0", path = "../nros", default-features = false }
  ```

* Layer 1 needs no ordering at all, so for it that edge is dead weight —
  measured in phase-340 W5.a as **16 units and 4 duplicated crates per
  invocation** (181 → 165 units, overlap 12 → 8), with byte-identical sizes.

The combination is the defect: the ordering edge is paid on **every** build so
that a **fallback** path can be a race instead of an error.

## What the fallback's stated justification is worth now

The doc comment justifies layer 2 as covering "custom-target JSON specs with
`[unstable] build-std` configs that don't propagate across `CARGO_TARGET_DIR`
boundaries". Checked 2026-08-06:

* **No custom JSON target specs exist in the tree.** Every `*.json` under a
  `target*` path is cargo's own (`.rustc_info.json`, fingerprints).
* The `build-std` users are NuttX (arm + riscv) and ESP32. NuttX is now handled
  **explicitly inside layer 1** (`-Z build-std=std,panic_abort` plus the patched
  `libc` injected by `--config`), precisely so it stops falling through.
* ESP32 (`build-std = ["core","alloc"]`) is NOT covered by that branch, which is
  gated on `target.contains("nuttx")`. Whether it currently reaches layer 2 is
  unverified — that is the one open question below.

So the original justification is largely stale, and the remaining case is a
single unhandled target family rather than a general capability gap.

## Fix shape

**A build must not guess.** Concretely (1-3 LANDED 2026-08-06; 4 outstanding):

1. **Never fall back silently.** If layer 1 fails, `panic!` with the cause and
   the remedy. A wrong `EXECUTOR_SIZE` is worse than a failed build; this is the
   same rule CLAUDE.md already applies to tests ("must fail on unmet
   preconditions") and the shape of issue 0351.
2. **Delete layer 3.** Committed size constants cannot track a type they do not
   observe. If a target genuinely cannot be probed, that target fails to build
   until layer 1 covers it.
3. **Make layer 2 deterministic or delete it.** The poll is only necessary
   because ordering is unguaranteed — but the build-dependency edge *does*
   guarantee it. Where the edge is present, a single lookup suffices; the
   timeout loop is redundant with the very edge that exists to serve it.
4. **Then make the edge optional** (phase-340 W5.b): off by default so the
   isolated path pays nothing, on for the target families that still need a
   guaranteed-ordered outer rlib.

Ordering matters: (1) must land before (3)/(4), so that removing a fallback
surfaces as a loud failure rather than a silent substitution.

## The backstop covers 2 of 15 macros

Checked 2026-08-06. Const assertions of the
`size_of::<T>() <= N * size_of::<u64>()` form exist for exactly two:

```
guarded:    EXECUTOR_OPAQUE_U64S, CPP_EXECUTOR_OPAQUE_U64S
unguarded:  ACTION_CLIENT, ACTION_SERVER, CPP_ACTION_CLIENT, CPP_ACTION_SERVER,
            CPP_GUARD_HANDLE, GUARD_HANDLE, NROS_CPP_RAW_ACTION_SERVER,
            NROS_LIFECYCLE_CTX, PUBLISHER, SERVICE_CLIENT, SERVICE_SERVER,
            SESSION, SUBSCRIPTION
```

So the "the const assertion catches it" reassurance holds only for the executor.
Thirteen opaque arrays have **no compile-time size check at all**, and layer 3
substitutes constants for several of them (`PUBLISHER_SIZE`, `SUBSCRIBER_SIZE`,
`SERVICE_CLIENT_SIZE`, `SESSION_SIZE`, `LIFECYCLE_CTX_SIZE`, the five `RAW_*`
sizes). For those, an under-sized value is not caught anywhere — it is simply a
short buffer that a C or C++ caller writes a larger Rust type into.

That makes the fallback chain a memory-safety concern rather than a build-hygiene
one, and it raises a second, independent work item: **every opaque macro should
carry the guard the executor has**, regardless of what happens to the probe.

There is a fourth committed artifact in the same chain:
`packages/api/nros-c/include/nros/nros_config_generated_nuttx.h` hardcodes
`EXECUTOR_OPAQUE_U64S 9912` — exactly `79_296 / 8`, agreeing with layer 3's
constant rather than with a measurement. The stale value therefore exists in two
committed places that corroborate each other.

## Why this rotted unnoticed: the gate was dead

`just verify-size-probe` — the script whose entire job is proving these sizes
don't flake — **had been failing at its first assertion**. Its `HEADER` pointed
at `packages/api/nros-c/include/nros/nros_config_generated.h`, which is a
Phase-119.3 **stub** containing zero `#define NROS_*_SIZE` lines; the real header
moved to `$CARGO_TARGET_DIR/nros-c-generated/`. So `extract_sizes` matched
nothing and, under `set -euo pipefail`, the script exited 1 before comparing
anything. Confirmed by running it unmodified on HEAD: `exit=1`.

It is in `group("debug")` and not part of any CI tier, so nothing noticed. Fixed
alongside this issue; the script now resolves the real header and fails loudly if
it is absent.

## A related path left alone, deliberately

`nros-cpp` emits, on a probe returning zero:

> `EXECUTOR_SIZE probe returned 0 — likely a cargo check --no-default-features
> run. The emitted CPP_EXECUTOR_OPAQUE_U64S will be 1; do not link the resulting
> rlib.`

Same shape as layer 3 — degrade and warn — but NOT the same judgement call: a
`cargo check` run genuinely has no rlib to probe, so hard-failing it would break
`just check`. It is left as-is. What is missing is enforcement: nothing prevents
linking an rlib built that way, the instruction lives only in a build-script
warning, and `CPP_EXECUTOR_OPAQUE_U64S = 1` is the most under-sized value
possible. Worth its own fix (a poison symbol, or a variant slug that fails to
resolve at link time — the mechanism issue 0360 already established).

## Open

* ~~Does the ESP32 `build-std` path reach layer 2?~~ **No.** Only `nros-c` and
  `nros-cpp` build scripts call the probe, and ESP32 is Rust-only — 3 rust
  fixture rows plus 1 rust workspace row, zero C/C++/mixed, and no esp32 leaf
  depends on either crate. So no esp32 branch was needed to remove layer 2.
* **NuttX remains UNVERIFIED for the removal.** It is the target layer 3 existed
  for, and it now panics rather than substituting. Layer 1 has an explicit NuttX
  branch (`-Z build-std` + the patched `libc`) added precisely so it stops
  falling through, so the expectation is that it works — but a cheap local
  verification was not possible: `nros-nuttx-ffi` needs `APP_MAIN_CPP` and then
  fails in cc-rs outside its cmake environment, before `nros-c` compiles for the
  NuttX target at all. **Run `just nuttx build-fixtures` before trusting this.**
  If the NuttX probe does fail, the result is a loud build failure rather than a
  short buffer — the intended direction, but a visible red lane.
