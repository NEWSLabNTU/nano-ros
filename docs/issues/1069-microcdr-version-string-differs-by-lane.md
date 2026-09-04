---
id: 1069
title: "`MICROCDR_VERSION_STR` compiles as 2.4.1 under CMake and 2.0.2 under cargo, against a tree that is 2.0.2 — three numbers, none authoritative"
status: open
type: bug
area: rmw, build
severity: low
found: 2026-09-05
related: [1068, phase-420]
---

# The version is restated by hand in four places

Measured during the phase-420 W9 survey:

| Where | Says |
| --- | --- |
| the `micro-cdr` gitlink | upstream **v2.0.2** |
| the `micro-xrce-dds-client` gitlink | upstream **v3.0.1** |
| `nros-sdk-index.toml` `[source.micro-xrce-dds-client]` | `2.4.3-nros1` |
| `nros-rmw-xrce/CMakeLists.txt:59-62` | `PROJECT_VERSION* = 2.4.1` |

The CMakeLists sets `PROJECT_VERSION*` for the XRCE client and never resets it
before the micro-CDR `configure_file` at line 117, so `MICROCDR_VERSION_STR`
compiles as `"2.4.1"` under CMake and `"2.0.2"` under cargo — for the same
vendored tree, which is neither.

## Severity: low, on evidence

`git grep` finds **zero readers** of those macros in either vendored tree or in
ours, so nothing behaves differently today. It is filed because the next reader
will believe one of them, and because four hand-written restatements of one fact
is the shape that produced issue 1068 next door.

## Fix

Derive it. The gitlink is the fact — `git -C <submodule> describe --tags` — and
everything else should read from one place, the way phase-420 W5 made rmw
descriptor fields derived rather than authored. Correcting the four literals
would leave the same four literals.
