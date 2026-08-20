---
id: 724
title: "#710's sink install made `nros_platform_log_write` a link requirement of every test binary"
status: resolved
opened: 2026-08-20
resolved: 2026-08-20
severity: high
area: [core, logging, testing, build]
related: [710, 708, 619, 589]
---

# #710's sink install made `nros_platform_log_write` a link requirement of every test binary

## Symptom

`cargo build --workspace --tests` — the compile half of `just ci` — fails on **69
test targets**, all with one error:

```
/home/aeon/repos/nano-ros/packages/core/nros-log/src/sinks.rs:72:
    undefined reference to `nros_platform_log_write'
/home/aeon/repos/nano-ros/packages/core/nros-log/src/sinks.rs:62:
    undefined reference to `nros_platform_log_flush'
```

62 in `nros-tests`, 7 in `nros-rmw-cffi`. Every tier is red: tier 1 cannot build
its suite, so nothing below it runs either.

Measured with `--keep-going`. **Without it the count reads as 7**, because cargo
stops scheduling after the first failures — which is how a partial set led me
twice to "fixed" conclusions about targets that had simply not been reached yet.

## Cause

`fe974d1e9` (issue 0710) made `dispatch_to_sinks` install the default sink on
first use instead of dropping the record:

```rust
if ptr.is_null() {
    init(sinks::default());          // <- reaches PlatformSink
    ptr = SINKS_PTR.load(Ordering::Acquire);
}
```

That is the right default — a board can no longer lose records by forgetting to
publish a sink list, which is the class 0708 kept failing to enumerate. But
`sinks::default()` is `PlatformSink`, and `PlatformSink` calls
`nros_platform_log_write`. So the extern went from *referenced only by code that
names the sink* to a **link requirement of anything that logs at all**.

A real image satisfies it through its `nros-platform-<rtos>` port. A host test
binary links no port, and now needs one.

`nros-log`'s own manifest had already written this hazard down, for
`platform-clock`:

> the extern is a link-time requirement on the final binary — every real image
> satisfies it via its `nros-platform-<rtos>` port, but host tools composing
> custom sinks without a platform port would not, **hence not a default**.

0710 made the platform sink a default without that escape, and the prediction
came true the same day.

Not a new class either — issue 0619 fixed exactly this for `nros-c` and
`nros-cpp`, and `nros-node` carries the same remedy. 0710 widened the blast
radius from "crates that name the sink" to "crates that log", which is nearly
all of them.

## Fix

The port comes to the test. Three placements, because the linkage rules differ:

| where | why there |
| --- | --- |
| `nros-tests` — dep made NON-optional + one anchor in `src/lib.rs` | all 62 integration tests link this lib, so one reference in the rlib carries the native lib to every one |
| `nros-tests/tests/logging.rs` — its own anchor | links `nros-log` directly, never `nros_tests`, so the lib anchor does not reach it |
| `nros-rmw-cffi` — dev-dep + an anchor in all 12 tests and `#[cfg(test)]` in the lib | each integration test is its own crate |

Two things the dependency alone does not do:

* **rustc drops an `--extern` nothing references**, and with it the build
  script's `link-lib`, so the symbols come back undefined with the dep present.
  Hence `extern crate nros_platform_cffi as _;`. `nros-node` records the same
  finding.
* **An optional dep cannot answer this.** `nros-tests` already had
  `nros-platform-cffi[posix-c-port]`, behind `trigger-test` / `loan-e2e` — both
  off in the default `--workspace` build that `just ci` runs.

The anchor is written in ALL test binaries of a crate, not only the ones that
log today. One spelling beats a list of which tests are allowed to log.

## What `nros-rmw-cffi` needed that `nros-c` did not

The issue-0619 remedy does not transplant. `nros-platform-cffi` dev-deps
`nros-node[rmw-cffi]`, which deps `nros-rmw-cffi` — so adding the dev-dep closes
a **cycle**. Legal, and `nros-log` already has it, but it moves feature
resolution:

* inheriting the full default feature set through the cycle broke four further
  targets outright (`E0432: no SlotLending in the root`);
* `default-features = false` fixed those and left `nros-rmw/lending` still
  dropped, so `loan_native` / `loan_fallback` / `rust_adapter` failed — targets
  that had been compiling **only because another workspace member happened to
  turn the feature on**.

Both are answered in the manifest: narrow the dep, and add a self-dep
(`nros-rmw-cffi = { path = ".", features = ["lending"] }`) so this crate's tests
say what they need rather than borrowing it. `nros-c` uses the same self-dep
shape.

## Verification

```
cargo build --profile nros-relwithdebinfo --workspace --tests --keep-going
```

69 failing targets -> `rc=0`, whole workspace test build clean.

## Not fixed here

The structural question 0710 leaves open: `nros-log` now makes a platform port
mandatory for any crate that logs, and each such crate pays with a dev-dep plus
a per-binary anchor. That is four crates so far (0619's two, `nros-node`, and
these two) and it will keep recurring — the next `no_std` crate to add a log
line gets the same undefined reference, and the failure names `nros-log`'s
source rather than the crate that must be edited.

A weak host-side default definition was considered and rejected: if the weak
object is extracted and the real port's archive member then is not, logging goes
silently no-op on a real image — reintroducing exactly the silent drop 0710
exists to prevent, in the configuration hardest to notice.

Worth a gate rather than more instances: the rule is checkable ("a crate whose
test targets link `nros-log` must have a port dep AND an anchor per test
binary"), and CLAUDE.md's own argument applies — four sites fixed by hand, one
at a time, is the shape that becomes a fifth.
