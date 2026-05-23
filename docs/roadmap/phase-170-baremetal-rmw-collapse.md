# Phase 170 — Absorbed by Phase 118

**Status.** Absorbed into archived tracker
`docs/roadmap/archived/phase-118-example-matrix-coverage.md`.

Bare-metal Rust example collapse was completed under **118.G**. Remaining
runtime transport issues moved to Phase 177.

## Original Scope

Collapse the per-RMW directory axis on the bare-metal Rust platforms:

```text
examples/qemu-arm-baremetal/rust/<case>/
examples/qemu-esp32-baremetal/rust/<case>/
examples/esp32/rust/<case>/
examples/stm32f4/rust/<case>/
```

RMW selection should happen through Cargo features rather than
`rust/<rmw>/<case>` directories.

## Remaining Work

None in this phase. Follow runtime transport issues in Phase 177.
