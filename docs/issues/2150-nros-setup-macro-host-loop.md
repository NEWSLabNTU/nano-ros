---
id: 2150
title: "`nros::main!` emits assembly and loop as one `main`, so a host that owns its own loop loses the `system.toml`-driven assembly entirely"
status: open
type: enhancement
area: [macros, core, executor]
severity: low
found: 2026-10-29
related: [2134, 2143, 2144, 2145, 2146]
---

## What

`nros::main!` emits two things as one unit:

- **assembly**, which every host wants: read the resolved model, install the
  components, apply the run plan and the deploy overlay, set up parameters,
  and register the deferred-dispatch slots;
- **the loop**, which differs per framework: a `fn main` for the board's entry
  shape, and the spin loop inside it.

There is no way to take the first without the second. `git grep` finds no
register-only or library-mode emit: the only entry macros are `nros::main!`
and `zephyr_component_main!`, and both emit an entry point. So a host that
owns its loop (an RFC-0112 Tier 0 host: Embassy, tokio, GLib, an Arduino
`loop()`, a PX4 work item) must hand-write the assembly that the model already
describes. That is the "Pattern A" ~150 lines in the book's Embassy chapter, and
it is the cost that makes demoting Embassy (issue 2134) look like a UX loss.

## Direction

Split the emit. Sketch, names not decided:

```rust
nros::setup!();   // emits `fn nros_setup(&mut Executor) -> Result<..>`, no main

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut executor = my_init(spawner);
    nros_setup(&mut executor).unwrap();
    spawner.spawn(nros_task(executor)).unwrap(); // host-owned loop (2143/2144)
}
```

- `nros::main!` becomes `setup` plus the board's loop, so the two cannot drift.
- Deferred dispatch (issue 2146) is solved inside the macro, which already
  emits the node ABI, rather than for hand-written code.
- One `nros_setup` serves every Tier 0 host, so admitting a new framework costs
  a recipe and a cell, never a macro branch. That is RFC-0112 D3's intent.

## Open questions

- Where the executor's backing comes from when the host opens the `Executor`
  (the `.bss` static `nros::main!` uses today, or caller-supplied).
- Whether tiered images (spawned tier executors) fit `setup`, or stay
  `nros::main!`-only.
- Whether this amends RFC-0112 (D2 or D4) or needs its own RFC.
