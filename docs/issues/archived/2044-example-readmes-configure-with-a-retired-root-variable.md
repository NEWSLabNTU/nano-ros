---
id: 2044
title: "Copy-out instructions configure with `NROS_REPO_DIR`, which `find_package(nano_ros)` never reads — the documented first build fails"
status: resolved
type: bug
area: [examples, docs, cmake]
severity: high
found: 2026-10-25
resolved: 2026-10-26
related: [rfc-0103, rfc-0026, phase-452, issue-1641]
---

# Copy-out instructions configure with a retired root variable

**Resolved 2026-10-26** — see the last section. **Home:** [phase-452](../../roadmap/phase-452-copy-out-front-door.md) W7.

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

## Resolution (2026-10-26)

One spelling everywhere a user is told what to type:

- **C/C++:** `cmake -S . -B build -Dnano_ros_ROOT=<root>`, or `nros build`.
  This is the hint `find_package(nano_ros)` reads. The fix is in
  `scripts/docs/gen-example-readmes.py`, and its 141 generated pages were
  re-rendered. Two of them, the `typed-carrier` pages, carry hand-written
  sections that `--force` would have dropped, so those two were patched in
  place. The same fix went into the examples index, the root `README.md` and
  the three `bins/*-poc` READMEs.
- **Rust:** `source <root>/activate.sh`, which puts that checkout's `nros` on
  PATH. The exported `NROS_REPO_DIR` reached no reader. This changed in the
  generator, the five templates, the examples index and RFC-0040's user flow.
- **The refusal:** `_nros_bootstrap`'s `FATAL_ERROR` names `nros build` and
  `-Dnano_ros_ROOT`.

**Gate.** `check-repo-dir-readers` now also reads fenced commands in tracked
Markdown. It refuses three forms: `export NROS_REPO_DIR=…`,
`NROS_REPO_DIR=… cmd` and `-DNROS_REPO_DIR=…`. Records (`docs/issues/`,
`docs/roadmap/`, changelogs) are exempt. Against origin/main's docs it reports
154 findings in 152 files; after the sweep it reports 0. Self-tests cover each
form, prose, and lines outside a fence.

**Measured on copies**, made with `env -i HOME=$HOME PATH=/usr/bin:/bin`:

- `examples/linux/c/action-client` with `-Dnano_ros_ROOT=<root>`:
  `-- Configuring done`.
- `examples/linux/rust/talker`, after `source <root>/activate.sh`: `nros sync`
  writes `build/linux/nros-cargo.toml`.

**Left as is**, because each describes rather than instructs:

- the leaf-block CMake variable in the PX4 README;
- `docs/reference/environment-variables.md`, which documents the variable that
  `just` recipes still export;
- RFC-0099 and `canonical-build-path.md`, which describe the resolution ladder.

