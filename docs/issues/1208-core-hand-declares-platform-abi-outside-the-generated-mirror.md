---
id: 1208
title: "Three core crates hand-declare 12 platform ABI symbols outside the
  generated mirror, so `check-platform-abi-mirror` watches a copy they do not use"
status: open
type: tech-debt
area: [core, platform, build]
related: [0555, 0160, 0196, phase-299, phase-352]
---

## What

RFC-0054 makes `packages/platform/nros-platform-api/include/nros/platform.h` the
SSoT for the platform C ABI, and `scripts/gen-abi-bindings.sh` generates the Rust
declarations into `packages/platform/nros-platform-cffi/src/generated.rs`.
`scripts/check-platform-abi-mirror.sh` gates the header against that generated
file plus the `nros_platform_export_*!` macros.

Three **core** crates do not use `generated.rs`. They hand-write their own
`unsafe extern "C"` declarations of the same symbols:

| file:line | symbols |
| --- | --- |
| `packages/core/nros-core/src/clock.rs:189-190` | `nros_platform_time_now_ns` |
| `packages/core/nros-log/src/lib.rs:663-664` | `nros_platform_clock_ns` |
| `packages/core/nros-node/src/executor/types.rs:1335-1336` | `nros_platform_time_now_ns` |
| `packages/core/nros-node/src/executor/spin.rs:9101-9102` | `nros_platform_clock_ns` |
| `packages/core/nros-node/src/executor/spin.rs:9127-9128` | `nros_platform_sleep_us` |
| `packages/core/nros-node/src/executor/node_wake.rs:45-54` | `nros_platform_wake_{init,drop,wait_ms,signal,signal_from_isr,storage_size,storage_align}` |

12 declarations, 5 files, 3 crates.

**The declarations are correct today.** Measured against
`packages/platform/nros-platform-api/include/nros/platform.h:164,318,374,683-692`
and `generated.rs:56,95,131,254-273`: every arity, argument type and return type
matches. This is a gate gap, not a live break.

## Why it is not simply a contract violation

The obvious reading — "core bypasses the platform seam, contrary to
ARCHITECTURE §2" — is wrong, and worth stating so the fix does not aim at the
wrong thing. The platform layer **is not a vtable**.
`packages/platform/nros-platform-cffi/src/lib.rs:24-30` says so:

> Platform sits one tier below RMW. The Phase 117 RMW vtable is a
> runtime-pluggable struct; the platform layer is link-time-bound free symbols.
> Different choice because RMW backends genuinely swap per session … while a
> platform is fixed for the life of a binary.

So calling `nros_platform_clock_ns()` *is* using the seam. And `nros-core` cannot
depend on `nros-platform-cffi` to reach `generated.rs` — it sits below it, which
`packages/core/nros-core/src/clock.rs:167-171` states outright. The dependency
half of the contract holds cleanly: no core crate depends on `nros-platform` or
any `nros-platform-<rtos>`.

The defect is narrower: **the SSoT has a generated mirror, and the heaviest
consumers of the ABI keep a second, hand-written mirror that no gate reads.**

## Why it matters

This exact shape has already cost two lane-stopping breaks, recorded in
`scripts/check-retired-platform-clock-symbols.py`:

> #547 — the Cyclone backend hand-declared the ABI in three per-platform
> `extern "C"` blocks — compiled fine, failed at LINK with
> `undefined reference to 'nros_platform_clock_ms'`
> #548 — the XRCE C shim, same shape, five undefined refs, and it took the whole
> tier-2 fixture build down

That script polices **retired names only**. A *signature* change to a live symbol
— say `nros_platform_wake_wait_ms` gaining an argument, or `nros_platform_sleep_us`
moving from `size_t` to `uint64_t` — regenerates `generated.rs`, passes
`check-abi-bindings` and `check-platform-abi-mirror`, and leaves core linking
against a stale prototype. On a `-> u64` vs `-> u32` change that is silent
garbage rather than a link error.

The repo already has the struct-side answer: `check-ffi-struct-mirrors`, filed
after the QoS `tx_express` / `callback_group` drift (issue 0160, three
occurrences). There is no function-side equivalent.

## Where the gate is narrower than the rule

`scripts/check-platform-abi-mirror.sh:29-31` scopes to exactly three paths:

```
RUST="packages/platform/nros-platform-cffi/src/lib.rs"
GENERATED="packages/platform/nros-platform-cffi/src/generated.rs"
INCLUDE_DIR="packages/platform/nros-platform-api/include/nros"
```

`packages/core/**` appears nowhere in it. This is the issue-0196 shape: a gate
whose coverage is narrower than the rule it enforces.

## Fix sketch (not applied)

Two candidate shapes, both structural rather than another grep:

1. **One declaration, reachable from below.** Move the hand-written block into a
   leaf crate that `nros-core` may depend on (or into `nros-platform-api`, which
   `nros-node` already depends on and which owns the header), generated the same
   way `generated.rs` is. Then there is one mirror and the existing gate covers
   it. This is the shared-helper answer CLAUDE.md prescribes over "a second
   spelling".
2. **Extend the mirror gate to every hand-declaration in the tree.** Harvest
   every `unsafe extern "C"` block declaring a `nros_platform_*` symbol anywhere
   under `packages/`, and require each declaration to match the header's
   signature textually. Catches the Cyclone/XRCE shape too, which recurred twice
   and is currently policed only for retired names.

Option 1 is preferable: it removes the duplicate rather than watching it.

## Sweep

```
rg -n 'fn nros_platform_' packages/core/*/src/
rg -n 'unsafe extern "C"' -A5 packages/ | rg 'nros_platform_'
```
