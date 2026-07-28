---
id: 273
title: "nros-board-mps2-an385-freertos nros-board.toml still advertises the retired `_start` entry signature"
status: resolved
resolved_in: "phase-313"
type: bug
severity: low
area: boards
---

## Resolution (phase-313, 2026-07-28)

Fixed the `nros-board.toml` `[board.entry] signature`: `_start() -> !` →
`#[unsafe(no_mangle)]\nextern "C" fn main() -> !`, matching `board_mps2.c`'s
`Reset_Handler → main` + the working fixtures (logging-smoke-freertos-mps2,
threadx-qemu-riscv64). Passes `check-board-manifest-drift`.

Two sibling instances of the same retired-`_start` shape found + fixed:
- `cargo-nano-ros` embedded scaffold stub (`scaffold.rs`) — emitted `_start` +
  a stale `run()` TODO; now teaches `main` + `Mps2An385::run_bare`.
- `wake-latency-cortex-m3` bench also carries `_start` — but it is **off-lane and
  rotted well beyond this bug** (phase-230 `platform-freertos` feature drift + a
  `nros-baremetal-common`/picolibc libc-stub duplicate-symbol link conflict), so
  its full resurrection is spun off to **#0313** rather than half-fixed here.

---

## Finding (autoware_sentinel phase-14 pin bump, 2026-07-25)

`packages/boards/nros-board-mps2-an385-freertos/nros-board.toml`:

```toml
[board.entry]
signature = "#[unsafe(no_mangle)]\nextern \"C\" fn _start() -> !"
```

but `c/board_mps2.c`'s `Reset_Handler` calls `main` — the `_start`
shape was retired (commit d99386173, the file's own comment block says
so). A legacy-`run()` consumer following the descriptor links with

```
rust-lld: error: undefined symbol: main
```

Update the descriptor's `[board.entry] signature` to the
`extern "C" fn main() -> i32` shape so codegen'd entries and
hand-written firmware agree with the C startup.
