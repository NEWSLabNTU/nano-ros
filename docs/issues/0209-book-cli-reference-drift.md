---
id: 209
title: "book/CLI reference drift: phantom `esp32` board id (setup fails), `nros init` verb undocumented"
status: open
type: bug
severity: low
area: docs
related: []
---

## Problem (audit 2026-07-16, F3/H3)

- `book/src/getting-started/installation.md:182` and
  `book/src/reference/cli.md:64` advertise board id `esp32`, but
  `nros-sdk-index.toml` defines only `qemu-esp32-baremetal` and
  `cmd/setup.rs` does no aliasing → documented `nros setup esp32` fails
  "unknown board".
- Top-level verb `nros init` (CMakePresets generator, RFC-0048 §6) has no
  section in `book/src/reference/cli.md`.

## Fix sketch

Rename the doc references to `qemu-esp32-baremetal` (or add an `esp32`
alias in the index if the short id is wanted); add a `### nros init`
reference section.
