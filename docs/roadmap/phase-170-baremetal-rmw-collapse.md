# Phase 170 — Absorbed by Phase 118

**Status.** Absorbed into
`docs/roadmap/phase-118-example-matrix-coverage.md`.

Bare-metal Rust example collapse is now tracked under **118.G**. Keep
active progress and checkboxes there so example-collapse ownership stays
in one place.

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

See **118.G** for the live checklist:

- qemu-arm bare-metal Zenoh and DDS/dust-DDS retirement decision.
- qemu-esp32 bare-metal Zenoh and DDS/dust-DDS retirement decision.
- real ESP32 Zenoh collapse.
- STM32F4 Zenoh collapse.
- board-specific feature proxying and fixture updates.
