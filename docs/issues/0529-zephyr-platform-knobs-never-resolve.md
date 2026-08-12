---
id: 529
title: "The zpico platform resolver can never select `zephyr`, so
  `config/zephyr/nros-platform.toml`'s knobs are unreachable"
status: open
type: bug
area: build-system
related: [issue-0460, issue-0135, phase-290, rfc-0049]
---

## The defect

`nros-zpico-build/src/runner.rs:398` picks the platform whose
`nros-platform.toml` supplies the RFC-0049 knob ladder:

```rust
let platform_name = if use_threadx      { Some("threadx") }
    else if use_nuttx                   { Some("nuttx") }
    else if use_freertos                { Some("freertos-lwip") }
    else if use_bare_metal              { Some("bare-metal") }
    else if !is_embedded_target(&target) && !use_system { Some("posix") }
    else                                { None };
```

**`use_zephyr` is absent**, and every Zephyr target matches
`is_embedded_target()` (it tests `target.contains("zephyr")`). So on Zephyr the
resolver always returns `None` and the ladder falls to its env-only branch,
taking `BUILTIN_TX_BATCH = false` / `BUILTIN_TX_SPLIT_LOCK = false`.

`config/zephyr/nros-platform.toml` is the **only** platform file in the tree
carrying `[knobs.zenoh.tx]` — `batch = true`, `split_lock = true`,
`flush_ms = 50`. Nothing reads it.

## Severity: latent, not live — and I got this wrong twice first

**Corrected before filing.** My first two write-ups of this claimed the phase-290
W5 promotion (measured 15–20× streaming) was silently inert on Zephyr, and then
that the C and Rust lanes disagreed as an issue-0135 ABI split. **Both are
false.** Recording the corrections because they were asserted in
[RFC-0072](../design/0072-rtos-integration-nano-ros-is-a-guest.md) §11 and
[phase-349](../roadmap/phase-349-rtos-integration-shells.md) before being
checked:

1. **The optimisation is applied on Zephyr.** It arrives by a different route —
   `zephyr/Kconfig:184` `NROS_ZENOH_TX_BATCH default y` and `:204`
   `NROS_ZENOH_TX_SPLIT_LOCK default y`, forwarded by
   `zephyr/cmake/nros_rmw_zenoh.cmake:76` as
   `zephyr_compile_definitions(ZPICO_TX_BATCH=1)`. The C lane is correct.
2. **There is no ABI split.** `build_c_shim()` is explicitly skipped on Zephyr
   (`runner.rs:569`: `if backend_count > 0 && !use_zephyr && !use_freertos &&
   !use_nuttx && !use_threadx`), so the config header the resolved knobs feed has
   no consumer there. And `rust_consts()` emits only the sizing constants
   (`ZPICO_MAX_{PUBLISHERS,SUBSCRIBERS,QUERYABLES}`) — `tx_batch` reaches no
   Rust code.

So today this costs nothing at runtime. What it actually is:

* **Two sources for one fact, agreeing by coincidence.** Kconfig says
  `y / y / 50`; the TOML says `true / true / 50`. Nothing enforces that, and
  nothing notices if one moves.
* **A config file that lies.** Editing `[knobs.zenoh.tx]` in
  `config/zephyr/nros-platform.toml` has no effect whatsoever, with no
  diagnostic.
* **A trap that springs later.** The moment `build_c_shim` is enabled on Zephyr,
  or a knob the Rust lane *does* read is added to that table, the resolver
  silently hands back builtins.

## Why the 0460 gate does not catch it

`check-kconfig-knob-forwarding` scans `zephyr/cmake/nros_cargo_build.cmake` for
`_nros_resolve_knob(<NAME>)` and requires each to be read by the Rust lane. The
tx trio is published from a *different* file — `nros_rmw_zenoh.cmake`, via
`zephyr_compile_definitions` — so it is outside the gate's view. The gate is
green and correct about what it looks at.

This is the issue-0196 rule again: when a gate exists for a class, check it
covers the new site.

## Fix

1. `use_zephyr => Some("zephyr")` in the chain, so the resolver is total over the
   platforms that have a config file.
2. A gate asserting the Zephyr Kconfig defaults and the platform TOML agree, so
   the two sources cannot drift apart silently. That is the part with lasting
   value — the resolver fix alone changes no behaviour.
