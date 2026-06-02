# nros-board-native

Tier-1 per-board shim for the `native` board (hosted Linux / macOS /
BSD). Defines a `NativeBoard` ZST that implements the
[`nros-platform`] `Board` trait surface (`BoardInit`, `BoardPrint`,
`BoardExit`, `BoardEntry`) by one-line delegation to
[`nros-board-posix`]'s `PosixBoard` family driver. POSIX libstd
handles every concern the embedded boards make explicit (clock, heap,
stdio, threading), so this shim has no per-board overrides — it exists
solely to give the tier-1 board name a dedicated crate, matching the
shape of `nros-board-mps2-an385-freertos`, `nros-board-stm32f4`, etc.,
and to keep the Phase 212.N.4 codegen emitter uniform
(`generate_single_node_main(NativeBoard)`).

[`nros-platform`]: ../../core/nros-platform/
[`nros-board-posix`]: ../nros-board-posix/
