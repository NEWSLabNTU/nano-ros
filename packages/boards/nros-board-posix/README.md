# nros-board-posix

POSIX (Linux / macOS / BSD) family driver crate for the `Board` trait
surface that lives in [`nros-platform::board`]
(`packages/core/nros-platform/src/board/`). Implements
`BoardInit`, `BoardPrint`, `BoardExit` and `BoardEntry` for a single
`PosixBoard` ZST so a host Entry pkg `main.rs` boots through the same
`<Board as BoardEntry>::run(setup)` shape every other family driver
uses (`nros-board-freertos`, `nros-board-threadx`, …).

POSIX is the simplest of the family drivers: libstd's runtime already
brings up the heap, stdio and threading before `fn main` runs, so
`init_hardware` is a no-op, there is no `TransportBringup` /
`NetworkWait` impl, and termination calls `std::process::exit`. The
executor open + spin lives inside the `setup` callback (typically the
codegen-emitted `run_plan(runtime)` from Phase 212.N.4) rather than
inside `BoardEntry::run`.

Consumers: host (`native`) Entry pkgs, Phase 212.N.4/N.5 codegen
`generate_single_node_main(PosixBoard)`, and any cross-target test
harness that wants the same `Board::run(setup)` shape on the host as
on the embedded targets.

[`nros-platform::board`]: ../../core/nros-platform/src/board/
