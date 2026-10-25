---
id: 2044
title: "Copy-out instructions configure with `NROS_REPO_DIR`, which `find_package(nano_ros)` never reads — the documented first build fails"
status: open
type: bug
area: [examples, docs, cmake]
severity: high
found: 2026-10-25
related: [rfc-0103, rfc-0026, phase-452, issue-1641]
---

# Copy-out instructions configure with a retired root variable

**Home:** [phase-452](../roadmap/phase-452-copy-out-front-door.md) W7.

## Problem

RFC-0103 D6 (amended 2026-10-12) retired `$NROS_REPO_DIR` and an exported
`nano_ros_ROOT` as INPUTS: a consumer finds nano-ros through `find_package
(nano_ros)` with the `-Dnano_ros_ROOT=<root>` hint, and `nros build` passes it.
`nano_rosConfig.cmake:32` sets `NROS_REPO_DIR` itself from its own location, so
a `-DNROS_REPO_DIR` on the command line cannot help `find_package` find that
file in the first place.

The copy-out instructions still say the old thing, and they fail.

## Evidence

The root README (`README.md:142-143`) and `examples/linux/c/action-client/README.md:10`,
followed exactly on a copied-out leaf with a minimal environment
(`env -i HOME=$HOME PATH=/usr/bin:/bin`, tree `4f545011ff`):

| command | result |
| --- | --- |
| `cmake -S . -B build -DNROS_REPO_DIR=<root>` (the README) | **FAIL** — `CMake Error at CMakeLists.txt:7 (find_package)` |
| `NROS_REPO_DIR=<root> cmake -S . -B build` (the README's "or: export") | **FAIL** — same |
| `cmake -S . -B build -Dnano_ros_ROOT=<root>` (RFC-0103 D6) | `-- Configuring done` |

Reach (`git grep -l … -- 'examples/**/README.md'`):

- 148 example READMEs name `NROS_REPO_DIR`; 76 give the `-DNROS_REPO_DIR`
  configure line, 146 an `export NROS_REPO_DIR`. Every tree: linux 45,
  rv-virt-threadx 20, qemu-armv7a-nuttx 19, mps2-an385-freertos 18,
  threadx-linux 18, mps2-an385-baremetal 13, zephyr 6, templates 5,
  esp32-c3-baremetal 2, px4 1, rv-virt-nuttx 1.
- Outside `examples/`: `README.md:142`, `docs/design/0040-distribution-and-scaffolding-deps.md:91`,
  three `packages/testing/nros-tests/bins/*-poc/README.md`.
- The refusal a user then meets teaches it too: `_nros_bootstrap`'s
  `FATAL_ERROR` (`cmake/NanoRosBootstrap.cmake:29-32`) says
  "Pass -DNROS_REPO_DIR=…, export NROS_REPO_DIR (source activate.sh)".
  RFC-0103 D6 says `activate.sh` puts `nros` on PATH and nothing else.

For a Rust leaf the `export` is dead rather than fatal (`nros` knows its own
root), so those READMEs mislead but do not break.

## Why no gate caught it

`check-repo-dir-readers` refuses CODE that reads the variable; documentation is
outside its population. `check-template-copy-out` builds templates by the
toolchain's own invocation (`nros build` / presets pass `nano_ros_ROOT`), not
by the README's text, and `just probe bootstrap` runs the book's tagged blocks,
not example READMEs.

## Fix direction

- One spelling in every README and doc: `nros build` for the leaf, or
  `cmake -S . -B build -Dnano_ros_ROOT=<root>` for a plain cmake configure; no
  `export`. Sweep: `git grep -l 'NROS_REPO_DIR' -- '*.md' ':!docs/issues/**' ':!docs/roadmap/**' ':!CHANGELOG.md'`.
- `_nros_bootstrap`'s refusal names `-Dnano_ros_ROOT` / `nros build`.
- Close the reach gap so it cannot come back: either extend
  `check-repo-dir-readers` to Markdown command lines, or make the copy-out
  probe run a README's own configure line (checklist G2: "DOCS are not gated,
  so read them").
