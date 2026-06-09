---
id: 13
title: stm32f4 talker-embassy fixture does not link
status: resolved
type: bug
area: build
related: [phase-226]
resolved_in: skip_build manifest flag
---

Surfaced by Phase 226.F: `build-test-fixtures` failed at the stm32f4 leaf —
`talker-embassy` had undefined symbols (`__assert_func`, `strncmp`,
`nros_platform_alloc`, …) on standalone `cargo build` plus duplicate
`platform_aliases` symbols in the shared fixture target dir. The pre-226
hardcoded recipe deliberately omitted it; the Phase 226 manifest migration
built every row, re-including the broken example.

Fixed (build no longer breaks): added a manifest `skip_build` flag (honored
in `fixtures-manifest.py::matches_filters` — excluded from both the build
list and the stale probe; surfaced in `fixture-inventory.py` as a
`skip-build` note) and marked `talker-embassy`
(`examples/fixtures.toml`). The example itself is still incomplete (does not
link standalone — missing board libc/platform glue + memory layout); fixing
that and dropping the `skip_build` flag is deferred follow-up.
