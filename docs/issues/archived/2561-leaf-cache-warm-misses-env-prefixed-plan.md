---
id: 2561
title: "The fixture builders' cache warm could not read a cargo plan line with env in front of it, fetched nothing, and the `--frozen` build failed on `failed to download rtic v2.3.0`"
status: resolved
type: bug
severity: medium
area: [build, ci, cargo, testing]
found: 2026-11-15
related: [0967, 1848, phase-466]
---

## What

The 2026-11-14 nightly (run 43559931051) failed on all six RTIC rows of
`QemuBaremetal` (`workspace-rust-baremetal-{talker,listener,service-*,action-*}-rtic`):

```
error: failed to download `rtic v2.3.0`
Caused by:
  attempting to make an HTTP request, but --frozen was specified
```

This is the headline `scripts/build/leaf-cache.sh` exists to prevent (issue
0967): before each `nros build --offline`, it reads the stage-5 command from
`nros build --dry-run` and runs `cargo fetch` against that generated root.

## Why

It found the command with `sed -n 's/^cargo build .*--manifest-path …/p'`.
The dry-run line is `Handoff::display()`, which prints the handoff's env
first. Since the stack-floor measurement put
`CARGO_TARGET_<T>_RUSTFLAGS=…`, `CFLAGS_<t>=…` and `RUSTC_WRAPPER=` on the
bare-metal cargo handoffs (NuttX's carries `RUSTUP_TOOLCHAIN=…`), that line no
longer starts with `cargo build`. The match found nothing, and
`[ -n "$manifest" ] || return 0` returned with nothing fetched and nothing
said. The stderr of the warm is not in the log, so the failure that followed
read as a network or lock problem.

A second, latent defect was in the same parse: `.*--config \([^ ]*\)` is
greedy, so on a line that also has `--config build.build-dir=…` it took the
LAST `--config` rather than the settings file.

## Fix (2026-11-15)

`nros_plan_cargo_build_args` finds the `cargo build` words wherever they sit
in the line (bare `cargo` or a path to it) and reads `--manifest-path` and the
first `--config` that names a file. A cargo image (`driver cargo` in its plan)
whose line still cannot be read now prints that nothing was fetched, instead
of returning silently. Fast gate `leaf-cache-plan-parse`
(`tests/leaf-cache-plan-parse-tests.sh`, 5 cases, built from the nightly's own
lines).

Sweep for other readers of the dry-run plan (only this one parses a cargo
line):

    git grep -n "\^cargo build" -- scripts just tests packages/testing
