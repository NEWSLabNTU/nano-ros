---
id: 779
title: "15 test files sit behind a crate-level `#![cfg(feature)]` that no recipe
  enables — cargo builds an EMPTY binary and nextest reports it green"
status: resolved
type: bug
area: testing, ci
related: [issue-0652, issue-0612, issue-0319, phase-376]
---

## Problem

Issue 0652 gated the case where a `[[test]]`'s `required-features` are off:
cargo skips the target silently, so it reads as coverage while running nothing.
`check-required-features-reachable` has kept that set EMPTY since.

There is a second spelling of the same lie, and the gate did not see it. A test
file whose first attribute is

```rust
#![cfg(all(feature = "lending", feature = "alloc"))]
```

still gets BUILT when the feature is off — it just compiles to a binary with no
tests in it. That is greener than `required-features`: nextest runs it and
reports success over zero cases, so even a lane that names the target looks
healthy.

Found 2026-08-24 while landing phase-376 W5. Six features, fifteen files, none
of them enabled by any recipe:

| feature | files |
| --- | --- |
| `lending` | `nros-rmw-cffi` loan_fallback, loan_native; `nros-rmw-zenoh` lending_traits |
| `posix-c-port` | `nros-platform-cffi` c_port_posix{,_critical_section,_net,_timer,_wake}, wake_wrapper |
| `c-stub-test` | `nros-platform-cffi` c_stub_platform; `nros-rmw-cffi` c_stub_transport |
| `bridge-stub` | `nros-rmw-cyclonedds` descriptor_seam, registry_race |
| `link-custom` | `nros-rmw-zenoh` custom_transport |
| `unix-mock` | `nvidia-ivc` loopback |

## Why it matters, concretely

`lending`'s two files had not merely stopped running — they had stopped
COMPILING. W3.d renamed `has_data` to take an out-parameter and moved the
return codes to named constants; both files still declared
`noop_hasd(_: *mut NrosRmwSubscription) -> i32` and used an unimported
`NROS_RMW_RET_ERROR`. Four tests, dead for a whole phase, in a directory that
reads as covered.

That is issue 0319's shape a third time: 0319 was a gate nobody ran, 0652 a
target no lane built, and this is a target every lane builds and none
populates.

## Fix

`check-required-features-reachable` now also scans crate-level `#![cfg(...)]`
in `packages/**/tests/*.rs` and requires each named feature to appear in a
`--features` position in some recipe — the same rule, over the same recipes,
for the second mechanism.

`lending` is wired and green (48 tests vs 44 without). The other five are a
dated BASELINE in that script, which is a shrinking backlog and not an
exemption: each needs its lane, or its files deleted. Several look like they
need more than a `--features` flag (`posix-c-port` and `c-stub-test` build C
stubs; `unix-mock` wants a loopback harness), which is why they are not being
switched on blind in the same commit that found them.

## Repro

```
python3 scripts/check-required-features-reachable.py   # lists all six
cargo nextest run -p nros-rmw-cffi --features alloc          # 44 tests
cargo nextest run -p nros-rmw-cffi --features alloc,lending  # 48 tests
```

## Backlog: 5 → 3 (2026-08-25) — and the feature named here is not enough

`bridge-stub` and `link-custom` are wired and out of the baseline. Five tests
that had never run now run, all passing:

| file | tests | crate-level cfg |
| --- | --- | --- |
| `descriptor_seam` | 1 | `#![cfg(feature = "bridge-stub")]` |
| `registry_race` | 3 | `#![cfg(all(feature = "std", feature = "bridge-stub"))]` |
| `custom_transport` | 1 | `#![cfg(all(feature = "platform-posix", feature = "link-custom"))]` |

`just check-required-features-tests` goes 26 → 31.

### The trap in this issue's own table

The table above lists ONE feature per file, and for two of the three that is not
what the file requires. Enabling exactly the named feature reproduces the bug
this issue exists to kill, one level in:

```
cargo nextest run -p nros-rmw-zenoh --features link-custom -E 'binary(custom_transport)'
  Summary  0 tests run: 0 passed, 0 skipped
  error: no tests to run
```

The target builds, runs zero tests, and a lane that named it would look wired
while covering nothing. Both were found this way — by running each feature alone
first and reading the count, rather than trusting the table.

So a lane must pass the WHOLE conjunction. Anyone retiring the remaining three
should check each file's `#![cfg(...)]` line directly, not this table, and
confirm the test count MOVES rather than that the command exits 0.

Nextest's default "no tests to run" ERROR is what makes that checkable — the
lane deliberately does not pass `--no-tests`, per the note already in the recipe.

### Remaining backlog (3)

* `posix-c-port` — 6 files, `nros-platform-cffi`. Builds C stubs.
* `c-stub-test` — 2 files, `nros-platform-cffi` + `nros-rmw-cffi`. Builds C stubs.
* `unix-mock` — 1 file, `nvidia-ivc`. Wants a loopback harness.

Each still needs its lane or its files deleted. None is a `--features` flag
alone, which is why they were not switched on blind with these two.

## Resolved (2026-08-25) — BASELINE is empty; all 15 files run

The remaining three are wired. `just check-required-features-tests` runs **73**
tests across every suite, none reporting zero, and
`check-required-features-reachable` reports `0 baselined backlog`.

| feature | files | tests | what it needed |
| --- | --- | --- | --- |
| `posix-c-port` | 6 | 38 | wiring only |
| `c-stub-test` | 2 | 3 | **repairs** — see below |
| `unix-mock` | 1 | 1 | `std` too |

### `c-stub-test` had rotted in two different ways

Neither was a flag.

**`c_stub_transport` did not COMPILE.** `build.rs` passed
`-I ../nros-rmw-abi/include`, which from `packages/rmw/cffi` names a directory
that does not exist; the header is at `packages/core/nros-rmw-abi/include`. Issue
0490 already corrected the *other* spelling in the same file — the
`rerun-if-changed` on line 122 — and left this one, with a comment 30 lines above
this call stating the correct path.

The two failed DIFFERENTLY, which is why only one was noticed: a missing
`rerun-if-changed` input is silently always-dirty and the build still succeeds,
while a missing `-I` is a hard `fatal error: nros/rmw_entity.h: No such file or
directory`. The hard one hid behind the disabled feature. So the ABI layout guard
this file exists to run had not compiled since phase-321 W2.e moved the crate — a
guard that cannot build is not a guard.

**`c_stub_platform` FAILED.** It asserted `counter(Time) >= 3`, but
`PlatformTime` has exactly one method (`time_now_ns`) and the test calls it once.
The trait was reduced and the expectation was not; `Total` was over by the same
2. Both corrected from the call list in the test body, not guessed. Also added a
`CffiPlatform::epoch_us()` call (issue 0758's new clock op) so the test's claim —
that EVERY category dispatches — stays true as the ABI grows.

### `unix-mock` was the third conjunction, and the only loud one

`--features unix-mock` alone fails with a `compile_error!` naming the remedy:
"`unix-mock` uses OS sockets: add \"std\" to this crate's features". Contrast
`link-custom` and `bridge-stub`, which built a target, ran zero tests and
reported green. A crate that refuses to compile without its companion feature
costs one confused minute; one that silently runs nothing costs a phase. The
fail-loud spelling is the better one and is worth copying.

### Standing rule

BASELINE is empty and should stay that way — a name added there is a test nobody
runs. If a file cannot be wired, delete it; that is also an answer, and an honest
one.
