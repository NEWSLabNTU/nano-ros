---
id: 273
title: "nros-board-mps2-an385-freertos nros-board.toml still advertises the retired `_start` entry signature"
status: open
type: bug
severity: low
area: boards
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
