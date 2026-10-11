---
id: 1811
title: "After phase-484 moved source trees into the store, four nightly RTOS port jobs still look for them in-tree or through retired exports"
status: open
type: bug
area: [ci, build]
severity: high
found: 2026-10-11
related: [phase-484, 1758]
---

## Measured

`nightly` run **38120811245** on `main` @ `90159b5d9` (schedule, 07:13Z
2026-10-11): 8 jobs passed, 5 failed and 3 were skipped. One of the failures
is the tier-2 nightly job, which stopped at the runner-label check (issue
1755's image rebuild, not this issue). The other four are the RTOS port jobs,
and all four fail the same way. Each provisions its sources into the **store**
(`~/.nros/sources/<name>/<version>+<sha8>`, phase-484 W3c `4f7ecbf06`), and a
later step then looks for them at the old in-tree path or through an export
that phase-484 W2c retired:

| Job | Step | Decisive line |
| --- | --- | --- |
| 114415327428 `freertos` | Build (freertos) | `build-examples: line 101: FREERTOS_DIR: unbound variable` |
| 114415327450 `qemu` | `just setup qemu` | `Error: zenoh-pico submodule not found` (printed right after `zenoh-pico … → /github/home/.nros/sources/zenoh-pico/1.7.2+739b1cc4 [provisioned]`) |
| 114415327482 `threadx_linux` | Build (threadx_linux) | `Skipping ThreadX Linux examples (ThreadX/NetX not found)` → `you named threadx-linux, so this is a FAILURE, not a skip` |
| 114415327501 `threadx_riscv64` | Build (threadx_riscv64) | same refusal for `threadx-riscv64` |

`just/freertos.just:41,101` still test `$FREERTOS_DIR/include`, and
`build-zenoh-pico` (`just/…:554`) still requires the in-tree submodule.
`git grep` finds 77 non-comment uses of `FREERTOS_DIR`, `THREADX_DIR` and
`NETX_DIR` across `just/`, `scripts/` and `activate.sh`.

## What it is NOT

- **Not issue 1758's rule misfiring.** The ThreadX jobs FAIL instead of
  skipping because they were named, which is 1758 working as intended. The
  missing tree is real.
- **Not the runner image (1755).** These four jobs run on GitHub-hosted
  runners inside the CI image and all reach their build steps.
- **Not caught earlier**, because no merge-gating lane builds these ports.
  The phase-484 W2c/W3c PRs passed the gate, and the first run that builds
  the ports is this nightly.

## What would close it

Each named recipe finds its tree the way phase-484 says every reader must:
`nros locate source <name>` (cmake / `just` / shell), never an in-tree path or
a retired export. `just/sdk-env.just`'s recipe-scoped values are the
documented exception (phase-484 §"just recipes keep their recipe-scoped
values"), so either `freertos.just` and the ThreadX recipes get their paths
from there, or `build-zenoh-pico`'s presence test asks `nros locate`. A gate
that refuses a new `$FREERTOS_DIR`/`$THREADX_DIR`/`$NETX_DIR` read outside
`sdk-env.just` keeps the class closed. Acceptance is the next 07:13Z nightly
building all four ports.
