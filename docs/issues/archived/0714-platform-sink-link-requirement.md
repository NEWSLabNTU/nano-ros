---
id: 714
title: "issue 0710's auto-install made a platform port a LINK requirement for every `nros-log` consumer"
status: resolved
type: bug
area: core, boards, platform
related: [issue-0710, issue-0708, issue-0589]
---

## What broke

Issue 0710 stopped searching for board boot funnels: `dispatch_to_sinks` installs
the platform sink itself when no sink list has been published, so no image can
silently drop records by forgetting `init`. That is the right shape, and it is
kept.

What it did not account for is that `PlatformSink` calls
`nros_platform_log_write` / `nros_platform_log_flush`, and the install sits on
the path EVERY record reaches. Referencing those externs from there turns a
platform port into a link-time requirement for every consumer of `nros-log` —
including host tools and the test harness, which have no port and never had one.
`packages/core/nros-log/Cargo.toml` already recorded this exact reasoning one
feature over, for `platform-clock`:

> the extern is a link-time requirement on the final binary — every real image
> satisfies it via its `nros-platform-<rtos>` port, but host tools composing
> custom sinks without a platform port would not, hence not a default.

Measured: after `fe974d1e9`, every `nros-tests` test target that links `nros-log`
without a port failed to LINK —

```
rust-lld: error: undefined symbol: nros_platform_log_write
  >>> referenced by sinks.rs:72
  >>>   <nros_log::sinks::PlatformSink as nros_log::LogSink>::log
```

`cargo check` does not link, so the whole class is invisible to a check-only
lane; it surfaces at `cargo build --tests` and at `test-all`.

## Fix

The auto-install is behind a new `nros-log` feature, `platform-sink`, default
OFF. The crates that SUPPLY the symbol turn it on, and cargo's feature
unification carries it into any image holding one:

* board crates under `packages/boards/` — a board IS the platform console, and
  the RTOS ports' symbols are C, linked at board level;
* `packages/platform/` crates whose `cffi-export` feature emits the canonical
  `nros_platform_*` symbols from Rust (mps2-an385, stm32f4, esp32-qemu);
* `nros-platform-cffi`'s `posix-c-port`, which compiles the POSIX C port.

A graph with no port neither gets the install nor needs it, and behaves exactly
as it did before 0710.

`nros-platform`'s own `cffi-export` is a SELECTOR — it forwards to each provider
crate's `cffi-export` — so it is credited by forwarding rather than made to
declare an `nros-log` edge for a symbol it does not supply.

## What this buys the gate

`check-board-log-sink.py` now checks the MANIFESTS rather than searching for boot
funnels, which is the first version of this rule that is actually checkable.
0708's rule was "every `pub fn run*` reaches `init_default()`", and it kept
missing funnels: NuttX's is `pub extern "C" fn nsh_main`, bare-metal's is
`#[entry] fn main()`, and three board crates did not link `nros-log` at all in
the configuration holding the funnel. A dependency row is finite, exact, and
cannot hide in a spelling.

Also fixed here: `nros-board-esp32-qemu/Cargo.toml` carried a DUPLICATE
`nros-log` dependency key (both added during 0708). `cargo` accepts it; strict
TOML parsers do not, which is how the gate found it.
