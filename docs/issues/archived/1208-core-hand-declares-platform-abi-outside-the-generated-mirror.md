---
id: 1208
title: "Three core crates hand-declare 12 platform ABI symbols outside the
  generated mirror, so `check-platform-abi-mirror` watches a copy they do not use"
status: resolved
type: tech-debt
area: [core, platform, build]
related: [0555, 0160, 0196, 0743, phase-299, phase-352]
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

## Resolution — the check, not the declaration (2026-09-10)

**The premise held: all 12 still match.** Re-measured against
`packages/platform/nros-platform-api/include/nros/platform.h` and
`nros-platform-cffi/src/generated.rs` before anything was written — every
arity, argument type and return type. Nothing had drifted, so this was closed
as the gate gap it was filed as, not as a live break.

The fix sketch above offered two shapes and preferred option 1, "move the
declaration into a crate `nros-core` may depend on, so there is one mirror".
**Option 2 landed instead, and the reason is the sweep.** The issue counted 12
declarations in 3 crates; the tree actually holds **94, across 31 files in 9
crates** — `nros-c`, `nros-cpp`, `nros`, `nros-board-nuttx`,
`nros-board-freertos`, `nros-smoltcp`, `nros-rmw-zenoh`'s three shims,
`zpico-sys`, `nros-bench`, `nros-platform-api` itself, and five of
`nros-platform-cffi`'s own C-port tests. Consolidating those into one crate
every one of them may depend on is not a refactor, it is a layering change
across the whole tree — and it would not even be correct for most of them,
since a `#[cfg]`-gated four-line block next to its one caller is the right
shape for a free-symbol seam. What was missing was never the single
declaration. It was the check.

### What the gate now covers

`scripts/check-platform-abi-mirror.sh` gains a closing section — the same gate,
wider scope, deliberately not a second tool. It calls
`scripts/lib/abi_hand_decls.py`, which harvests every `fn <prefix>*`
declaration inside an `extern "C" { }` block under `packages/` + `examples/`
and compares each against `generated.rs` positionally: argument types and
return type, names discarded.

`scripts/check-board-abi-mirror.sh` gains the same section, because the module
is parameterised over SURFACES rather than hardcoded to the platform. The board
surface checks **0** declarations today and exempts 3 board-local symbols — an
honest zero, and the gate is there so the first real mirror is checked rather
than noticed later.

The **RMW surface is deliberately not wired**, and the reason belongs in the
record: that seam IS a runtime vtable (`NrosRmwVtable`), so a backend reaches it
through a struct, and `check-rmw-abi-shape` / `check-rmw-api-parity` already
answer the per-slot question. The `nros_rmw_*` free symbols that exist are
backend REGISTRATION entry points, which live in no ABI header.

Four spellings compare EQUAL, each for a stated reason rather than to make the
tree pass:

| spelling | why it is one type |
| --- | --- |
| `core::ffi::c_void` vs `c_void` | path prefix |
| `Option<extern "C" fn(..)>` vs `extern "C" fn(..)` | bindgen models C NULL through `Option`; Rust guarantees the null-pointer optimisation |
| `nros_platform_timer_callback_t` vs the fn pointer it names | typedef, resolved FROM `generated.rs`, not hardcoded |
| `*const c_char` vs `*const u8` | `c_char` is `i8` on x86_64 and `u8` on arm, so no fixed spelling is portable — normalised ONLY behind a pointer, so a bare `-> i8` and `-> u8` stay different |

One more: a hand mirror may write `-> !` where bindgen wrote `-> ()`, but only
for a symbol the HEADER marks noreturn. That set is READ from the headers
(`nros_platform_panic`, and nothing else), because bindgen runs without
`--enable-function-attribute-detection` on this surface and `!` is then the
more faithful rendering, not drift.

A hand-declared symbol that `generated.rs` does not carry is an ERROR unless it
is in the surface's `out_of_surface` map WITH a reason — which is the #547 /
#548 retired-name shape caught generically, one layer above
`check-retired-platform-clock-symbols.py`'s name list. Four platform symbols
are exempt (`nros_platform_zephyr_wait_network`, `_freertos_seed_rng`, two
stub counters). The map cannot rot into a bypass list: an exemption whose
symbol has since JOINED the header is reported as stale.

### Two failure modes the gate refuses to have

**It cannot pass by matching nothing.** A harvest of zero declarations, or a
`generated.rs` that parses to zero, exits 2 — not 0. And every `fn <prefix>*`
the harvester declines to parse and cannot show is a definition is reported as
UNCHECKED, naming the file and line. That is the issue-0196 rule turned on this
gate itself: coverage narrower than the rule is the defect being fixed here, so
a silent parse failure would reintroduce it.

**It cannot pass by being unable to fail.** `--self-test` runs on the NORMAL
path (not behind a flag) at the head of both gates, with eight synthetic cases
asserting BOTH directions: five drifts that must be caught, three spellings
that must not be — so a normaliser tightened into uselessness fails too.
Mutation-tested by collapsing every type to a constant: the self-test went red
and took the gate with it.

### Mutation tests

Return type first, since that is the direction with no link error behind it.

| mutation | pre-1208 gate | new gate |
| --- | --- | --- |
| `nros-log/src/lib.rs:714` `nros_platform_clock_ns() -> u64` → `-> u32` | **green** | red, naming `packages/core/nros-log/src/lib.rs:714` and both signatures |
| `node_wake.rs:48` `wake_wait_ms(w, timeout_ms)` loses its `u32` | green | red, naming the file, line and symbol |
| comparator's type normaliser collapsed to a constant | n/a | red, self-test, before the sweep runs |

`check-retired-platform-clock-symbols.py` was also green on the return-type
mutation, which is the point: it polices names, and no name changed.

Both mutations restored; the tree is clean.

### Also landed

`.config/gate-selftest-baseline.txt` loses both scripts — the ratchet may only
shrink, and both now run their negative control on the normal path (111 → 109
scripts still owing one).

### Not done, deliberately

The 94 hand declarations stay where they are. `nros-core/src/clock.rs` is right
that it sits below `nros-platform-cffi`, ARCHITECTURE §2 is not violated by a
crate calling a free symbol the platform seam is DEFINED as, and the dependency
half of the contract already held cleanly. Watching the mirrors is the fix;
deleting them would have been a layering change wearing a cleanup's clothes.

## Fix sketch as filed (kept — the resolution above says which arm landed and why)

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

As filed (finds the 12, misses the other 82):

```
rg -n 'fn nros_platform_' packages/core/*/src/
rg -n 'unsafe extern "C"' -A5 packages/ | rg 'nros_platform_'
```

As it runs now — the gate IS the sweep, and `--list` is the audit view:

```
just check platform-abi-mirror
just check board-abi-mirror
python3 scripts/lib/abi_hand_decls.py platform --list   # 94 decls, 31 files
python3 scripts/lib/abi_hand_decls.py board --list
python3 scripts/lib/abi_hand_decls.py --self-test       # the negative control
```
