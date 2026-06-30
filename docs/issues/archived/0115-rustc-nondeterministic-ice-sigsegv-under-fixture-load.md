---
id: 115
title: "Non-deterministic rustc ICE / SIGSEGV under heavy parallel fixture-build load"
status: resolved
type: bug
area: build
related: []
---

## Summary

On at least one dev host, `just build-test-fixtures` (and therefore `just ci`'s
`test-all` / `cyclonedds-ci`) intermittently fails because **rustc crashes** — either
a hard `SIGSEGV` or an internal-compiler-error / `the compiler unexpectedly panicked`
— while compiling some crate. The failing crate is **different on every run** (observed:
`paste` build script, `toml`, `nros-macros`, `nros`, `nros-cpp`), and re-running the
build always advances past the crate that crashed last time. Example query stack:

```
query stack during panic:
#0 [optimized_mir] optimizing MIR for `...TierRtosSpec::deserialize::__Visitor::visit_seq`
#1 [items_of_instance] ...visit_seq::<&mut toml::value::SeqDeserializer>
error: could not compile `nros-macros`
```

## Not the obvious causes

Ruled out by investigation on the affected host:

- **Not OOM** — 94 GiB RAM, ~89 GiB free during the build; no swap needed; no kernel OOM
  kill. The crash is a SIGSEGV/ICE inside rustc, not an allocation failure.
- **Not sccache** — sccache is not installed on this host (`RUSTC_WRAPPER` resolves empty
  there), so it is not a cache-corruption issue.
- **Not the parallel front-end** — no `-Z threads` / parallel-compiler config anywhere.
- **Not reproducible in isolation** — 24 concurrent *fresh-target* `nros-macros --release`
  builds (far more rustc concurrency than the fixture build) produced **zero** crashes.
  The crash only appears in the fixture build's specific mixed load (cargo + cmake + ninja +
  nested sizes-probe / corrosion cargos, persistent target dirs, the `nros-fast-release`
  profile with `incremental = true` + `codegen-units = 16`).

The evidence points to a **non-deterministic rustc-1.96.0 crash** triggered by the host's
heavy mixed-build conditions (marginal toolchain/host behaviour under sustained load and/or
incremental-cache state) — not a defect in nano-ros source. It is environmental and host-
specific, so it cannot be fixed in rustc from here; the correct response is to make the build
**resilient** to it.

## Resolution

Added `scripts/build/rustc-retry.sh`, wired as the `just` `RUSTC_WRAPPER` fallback when
sccache is absent (`justfile`). cargo invokes it as `rustc-retry.sh <rustc> <args…>`; it:

- buffers each attempt's stdout/stderr and emits only the final attempt's, so a retried
  crash never double-feeds cargo's stdout artifact-JSON stream;
- retries **only** on a crash signature — exit `139`/`134`/`135`/`136`/`132` (128 + fatal
  signal) or exit `101` whose stderr carries `internal compiler error` / `unexpectedly
  panicked` / `rustc interrupted by SIG…`. A normal compile error (exit 101 without an ICE
  signature) is forwarded **immediately** and never retried, so real failures still fail fast;
- bounds retries with `NROS_RUSTC_RETRY` (default 3); `NROS_RUSTC_RETRY=1` disables it.

Because the crashes are non-deterministic (a re-run always advances), a bounded per-rustc
retry transparently recovers. Unit-tested: transparent passthrough on success, fail-fast on a
real `E0001`-style error (no retry), retry-then-succeed with a single clean stdout, and
cap-out on a persistent SIGSEGV.

## Scope / residual

This covers the **rustc** side (cargo's `RUSTC_WRAPPER`) — by far the most frequent failure
across the observed runs (random rustc ICE/SIGSEGV on paste, toml, nros-macros, nros, nros-cpp).
The crash mechanism is unit-verified; it acts only on actual crashes.

The **same host flakiness also occasionally crashes the C/C++ linker** — observed once as
`collect2: fatal error: ld terminated with signal 11` at Zephyr's final `zephyr.elf` link. That
link step runs inside Zephyr's own CMake rules with **no cargo- or launcher-level hook** the way
rustc has `RUSTC_WRAPPER` (CMake's `*_COMPILER_LAUNCHER` wraps compile, not link), so a
cargo-level wrapper cannot reach it. It is rarer than the rustc crashes and a re-run advances
past it. A general fix would need a PATH-level retrying `cc`/`ld` shim, which is invasive and
out of scope here.

**Bottom line:** the root cause is a flaky host that randomly SIGSEGVs build tools under heavy
mixed load — not a nano-ros defect. The rustc wrapper removes the dominant failure mode; the
residual C/C++ link crash is documented and host-specific. This is a mitigation, not a toolchain
fix: if a future toolchain/host stops crashing, the wrapper is a harmless passthrough, and when
sccache is on PATH it still wins (unchanged behaviour). Host-specific build notes live in agent
memory `box-fixture-sizes-probe-sigsegv`.
