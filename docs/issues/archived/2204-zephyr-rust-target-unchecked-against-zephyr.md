---
id: 2204
title: "A Zephyr board's `rust_target` is never checked against the triple zephyr-lang-rust picks"
status: resolved
resolved_in: "2026-11-01"
type: bug
area: [zephyr, build, cli]
severity: low
found: 2026-11-01
related: [2151, 2177, 2199]
---

## What was measured

An out-of-tree board descriptor for the Seeed XIAO RP2350
(`west_board = "xiao_rp2350/rp2350a/m33"`) stated
`[board.zephyr] rust_target = "thumbv8m.main-none-eabihf"` and the same triple
in `[board.provisioning] rust_targets`. The Zephyr 4.4 Rust build compiled
for `thumbv8m.main-none-eabi`:

```
-- Building Rust llvm target thumbv8m.main-none-eabi
```

zephyr-lang-rust's `_rust_map_target` picks `-eabihf` for a Cortex-M33 only
with `CONFIG_FP_HARDABI`, and the board's defconfig leaves the FPU off.
Nothing reported the disagreement. The image built because the `-eabi`
target happened to be installed. On a host with only the descriptor's triple
(what `nros setup` installs, issue 2151) the build would have failed deep in
cargo, with no mention of the descriptor.

## Cause

There are two statements of one fact, and nothing compared them. The
descriptor's triple drives provisioning and the preflight
(`builder/preflight.rs`, `cmd/setup.rs`). The build's triple comes from the
image's Kconfig at configure time. On Arm the FPU ABI is a Kconfig choice, so
the two can differ for any M4/M7/M33 board.

## Fix

- `nros ws board-facts --rust-target <triple>` checks instead of printing.
  It resolves the same deploy's board and refuses when the board's
  `rust_target` (with `{host_arch}` substituted) is a different triple. The
  refusal names both triples, the descriptor file, and the Kconfig choice
  behind the build's triple. A descriptor that states no triple is not
  checked.
- The Zephyr module schedules `nros_check_zephyr_rust_target()` for the end
  of the configure (`cmake_language(DEFER)`). The check runs only for a Rust
  image (`CONFIG_RUST`). It calls zephyr-lang-rust's own `_rust_map_target`,
  not a copy of its table, and then asks the CLI the same question
  `nros_resolve_board_facts` resolved, adding `--rust-target`.

This is a refusal and not a warning: the descriptor's triple is what
provisioning installs, so a wrong one is a broken install that only works by
accident.

The four in-tree descriptors that state a triple (`mps2-an385-zephyr`
`thumbv7m-none-eabi`, `qemu-cortex-a53` and `fvp-aemv8r-smp`
`aarch64-unknown-none`, native_sim `{host_arch}-unknown-none`) match the
mapping their Kconfig selects.

Test: `cmd::board_facts::tests::a_zephyr_rust_target_disagreement_is_named`.

The full path was measured on the XIAO leaf with Zephyr 4.4 and
`west build --cmake-only`:

| Image | Result |
|---|---|
| Rust, defconfig | refused, naming `thumbv8m.main-none-eabihf` vs `thumbv8m.main-none-eabi` |
| Rust, `CONFIG_FPU=y` + `CONFIG_FP_HARDABI=y` | configures (`Building Rust llvm target thumbv8m.main-none-eabihf`) |
| C, same board | configures (no `CONFIG_RUST`, no check) |
