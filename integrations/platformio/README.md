# nano-ros PlatformIO adapter (Phase 212.H.6)

> **STATUS: NOT A SUPPORTED INTEGRATION (2026-08-20).**
>
> Nobody is maintaining this, and as of issue 0704 nothing tests it: the e2e
> bringup suite, its workspace fixture and the `just platformio` recipes were
> removed rather than left as an opt-in nobody runs. "Present but untested"
> is a holding position, and this tree keeps deleting the shape where a test
> could not fail.
>
> **What is kept, and why.** The two files here — this README and
> `nros_codegen.py` — plus the repo-root `library.json` are the whole adapter,
> and they are DATA rather than a build path: nothing in `just ci` reaches
> them, so they cost nothing to keep and they are what someone picking this up
> again would otherwise have to re-derive. The CLI and board side stays too
> (`[deploy.<target>].framework` passes through, and
> `nros-board-esp32-qemu` names PlatformIO in its descriptor) because those
> are live code paths shared with supported targets.
>
> **Resuming it means** re-testing it properly: the `pio run` that made the
> old test slow has to become a BUILD-stage fixture, not a run-time
> compilation — CLAUDE.md's "No compilation inside tests" — which is the real
> reason the suite blew a 60 s budget on a cold package cache.

PlatformIO has no configure-time hook rich enough to read `system.toml`
from inside its tool, so this adapter follows the **ahead-of-vendor**
path documented in `docs/design/0003-rtos-integration-pattern.md` §1: a PIO
pre-build `extra_script` invokes `nros codegen-system
--ahead-of-vendor` BEFORE PIO's library resolver sees the tree.

## User incantation

Add nano-ros to your `platformio.ini`:

```ini
[env:my_board]
platform = espressif32
board = esp32dev
framework = espidf
lib_deps =
    nano-ros = {git = "https://github.com/NEWSLabNTU/nano-ros.git"}
build_flags =
    -DNROS_BRINGUP_NAME=demo_bringup
```

Then:

```
pio run -e my_board
```

The pre-build script bakes `<workspace>/<bringup>/system.toml` +
`launch/*.xml` into `${PIO_BUILD_DIR}/nros-system/{include,src}/` and
injects them into the environment's `CPPPATH` + `SRC_FILTER`.

## Files

- `../library.json` (repo-root) — PIO library manifest; declares this
  script as the `build.extraScript`.
- `nros_codegen.py` — ~50 LoC pre-build hook (PIO requires Python).
- `README.md` — this file.

## Required environment

- `NROS_BRINGUP_NAME` (via `build_flags = -D…` or env) — bringup pkg name.
- Optional `NROS_BIN` — explicit `nros` path (default: PATH lookup, then
  the in-tree `packages/cli/target/release/nros`, then `~/.nros/bin/nros`
  as a transitional fallback).
- Optional `NROS_WORKSPACE` — explicit workspace root (default: PROJECT_DIR).

## LoC budget

`tokei integrations/platformio/` reports < 100 LoC. The repo-root
`library.json` adds ~20 lines of JSON. Total adapter surface ≤ 200 LoC
per the Phase 212 §H.8 budget.
