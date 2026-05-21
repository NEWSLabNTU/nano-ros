# Phase 167 — Absorbed by Phase 118

**Status.** Absorbed into
`docs/roadmap/phase-118-example-matrix-coverage.md`.

The NuttX Rust collapsed-shape link regression is now tracked as
**118.E.4**. Keep the active checklist, acceptance criteria, and progress
updates there so example-collapse ownership stays in one place.

## Original Scope

Restore `cargo build` on collapsed-shape
`examples/qemu-arm-nuttx/rust/<case>/` directories. The depth-4 collapsed
layout failed with:

```text
undefined reference to __libc_init_array
undefined reference to __libc_fini_array
```

while the legacy depth-5 layout
`examples/qemu-arm-nuttx/rust/zenoh/<case>/` linked with the same
toolchain/config.

## Remaining Investigation

See **118.E.4** for the live checklist:

- `build-std` libc patch scope.
- newlib/libgloss startup selection.
- emitted `-nostartfiles` / `-nodefaultlibs`.
- fixture path and just recipe migration once the link fix lands.
