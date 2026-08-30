# nros-board-linux

Linux host board crate — the POSIX platform family's host driver — for
the `Board` trait surface that lives in [`nros-platform::board`]
(`packages/platform/nros-platform/src/board/`). Implements
`BoardInit`, `BoardPrint`, `BoardExit` and `BoardEntry` for a single
`LinuxBoard` ZST so a host Entry pkg `main.rs` boots through the same
`<Board as BoardEntry>::run(setup)` shape every other family driver
uses (`nros-board-freertos`, `nros-board-threadx`, …).

The REACH is `linux`, not `posix`: `apply_tier_affinity` calls
`sched_setaffinity` with `cpu_set_t` / `CPU_SET`, which libc does not
define for apple, and the call is not `cfg(target_os)`-gated — so this
crate does not build on macOS. The PLATFORM beneath it
(`nros-platform-posix`) is POSIX-clean; the two layers are named
separately on purpose.

This is the simplest of the family drivers: libstd's runtime already
brings up the heap, stdio and threading before `fn main` runs, so
`init_hardware` is a no-op, there is no `TransportBringup` /
`NetworkWait` impl, and termination calls `std::process::exit`. The
executor open + spin lives inside the `setup` callback (typically the
codegen-emitted `run_plan(runtime)` from Phase 212.N.4) rather than
inside `BoardEntry::run`.

Consumers: host (`native`) Entry pkgs, Phase 212.N.4/N.5 codegen
`generate_single_node_main(LinuxBoard)`, and any cross-target test
harness that wants the same `Board::run(setup)` shape on the host as
on the embedded targets.

[`nros-platform::board`]: ../../core/nros-platform/src/board/
