---
id: 2151
title: "`nros setup zephyr` does not install the Rust target a Zephyr Rust image compiles for"
status: open
type: bug
area: [cli, setup, zephyr]
severity: medium
found: 2026-10-30
related: [1672, 2149, phase-517]
---

## What

A Rust image on Zephyr native_sim/64 compiles for `x86_64-unknown-none`.
zephyr-lang-rust picks that triple from the Zephyr board at build time. On a
host where only `nros setup zephyr` ran, the build fails:

```text
error[E0463]: can't find crate for `core`
  = note: the `x86_64-unknown-none` target may not be installed
```

That was measured on phase-517 W17's `zephyr-rust` installed probe track
(`probe-platforms` run 40910156964): `nros new <ws> --workspace --lang
rust`, then `nros new entry zephyr_entry --platform zephyr`, `nros sync`,
`nros build`.

Three places know about the triple, and none of them installs it on this
path:

* **The index declares it.** `[rust.target.x86_64-none]` (issue 1672) says
  it is "picked by zephyr-lang-rust at build time, so it was declared
  nowhere".
* **The board descriptor does not.** Only `fvp-aemv8r-smp` states
  `[board.provisioning] rust_targets`, and only `nros setup board <name>
  --zephyr-workspace` reads that field.
* **Neither path covers it.** `nros setup zephyr` provisions the index row
  `[board.zephyr]`, whose `packages` is `zephyr-sdk` alone. The
  `nros build` preflight checks a board's `target`, and a Zephyr board has
  none, because west decides it.

## Workaround

The chapter's Rust block runs `rustup target add x86_64-unknown-none`
before the build (`book/src/getting-started/integration-zephyr.md`).

## Fix direction

* State the triple each Zephyr board's Rust build compiles for once, in the
  board descriptor (`[board.provisioning] rust_targets`). native_sim/64 →
  `x86_64-unknown-none`, mps2-an385 → `thumbv7m-none-eabi`, and so on.
* Have both `nros setup zephyr` and `nros setup` at a workspace root
  (phase-517 W4) install them when the workspace has a Rust Zephyr image.
* Have the `nros build` preflight check them the way it checks a cross
  board's `target`.
