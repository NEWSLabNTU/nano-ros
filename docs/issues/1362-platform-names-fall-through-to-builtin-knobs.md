---
id: 1362
title: "Five platform names the tree builds are answered by no
  `nros-platform.toml`, so each takes the BUILTIN knob defaults instead of its
  own — silently, three warnings at a time"
status: open
type: bug
area: build, boards
severity: medium
found: 2026-09-13
related: [1145, 0196, phase-448]
---

## What this is

A platform descriptor's `names` is the list of names that platform answers to;
the directory it lives in is only where the file sits. A name no descriptor
lists is an `UnknownPlatform`, and `or_builtin_rungs`
(`nros-platform-config/src/platform_config.rs`) turns that into a
`cargo:warning` plus the BUILTIN knob defaults — three times per build, once
each for the executor, params and memory rungs.

That fall-through is deliberate and the comment says why: it keeps a TYPO in
`NROS_PLATFORM_NAME` visible instead of silently selecting builtins. Right for
a typo. Wrong for a platform that simply never declared its own name, because
the builtins it lands on are not that platform's numbers.

## Measured 2026-09-13

`NROS_PLATFORM_NAME=<name> cargo check -p nros-node`, with `build.rs` touched
so the script re-runs:

| name | fall-through warnings |
| --- | ---: |
| `esp32` | 3 |
| `baremetal` | 3 |
| `nuttx-riscv` | 3 |
| `freertos-posix` | (same shape) |
| `zephyr-cortex-m` | (same shape) |

All five appear as `platform = "…"` rows in `examples/fixtures.toml`, so each
is a platform the tree actually builds.

## Why this was worth its own issue rather than a five-line fix

Issue 1145 hit the SAME class one name over: `threadx-linux` fell through, took
builtin executor knobs below `EXECUTOR_BACKING_DEFAULT_U64S`, and the backing
guard then refused the reservation — so the platform stopped COMPILING, in a
nightly, with an error naming neither the descriptor nor the name. That one is
fixed (the threadx descriptor answers to its two variants now).

Fixing the other five is **not** the same edit. Adding a name to a descriptor
swaps builtin knob values for that descriptor's, which is a behaviour change per
platform:

* `nuttx-riscv`, `freertos-posix`, `zephyr-cortex-m` each have a plausible
  parent descriptor, and adding the name is probably right — but "probably" is
  the word that turns one fixed regression into four new ones. Each wants its
  knobs compared before and after, on that platform's own image.
* `esp32` and `baremetal` have **no platform package at all**. For them it is
  not a missing name, it is a decision: do they get a descriptor, or is
  falling through to builtins the correct answer for a platform with no RTOS
  to describe?

## Gated meanwhile

`check-platform-name-answered` (fast line, buildless) asserts that every
`platform = "…"` in `examples/fixtures.toml` is answered by some descriptor's
`names`. These five are a RATCHET — `BASELINE_UNANSWERED`, which may only
shrink — so a SIXTH cannot appear unnoticed while these wait.

The gate is mutation-tested: reverting the threadx fix makes it report both
threadx names, and a baselined name that starts resolving is reported STALE
rather than silently kept.

## Fix

Per name, in this order: compare the knobs that platform gets today (builtins)
against the ones it would get from the candidate descriptor, on a built image;
then either add the name and drop it from the baseline, or record why
falling through is correct for it. `esp32` and `baremetal` need the decision
above first.
