---
id: 1359
title: "All 22 nightly `zephyr` jobs die in `setup`, not in Zephyr provisioning: the
  container has no `unzip`, so `nros setup --tool clang-format` cannot unpack its
  prebuilt — and the step NAME says `Set up Zephyr … workspace`, which is how this got
  read as issue 1158 for days"
status: open
type: bug
area: ci, tooling
severity: high
found: 2026-09-13
related: [1158, 0368, 1353]
---

## What happens

Nightly run **34739657223** (schedule, 2026-09-13T05:10) has 23 failing jobs. Twenty-two
of them are the `zephyr *` matrix, every one failing at the step named
`Set up Zephyr 3.7 workspace` / `Set up Zephyr 4.4 workspace`. The step name is the
only thing about them that concerns Zephyr. Job **103677080644**
(`zephyr 3.7 / rust/talker`) is representative:

```
nros setup --tool clang-format: prebuilt 17.0.6-nros1 (dist linux-x86_64) → /github/home/.nros/sdk/clang-format/17.0.6-nros1
  [FAILED]  clang-format — not installed — needs system package(s) this host is missing: unzip
Error: 1 package(s) failed to install (see [FAILED] above)
Location:
    nros-cli-core/src/cmd/setup/session.rs:837:23
error: recipe `setup-clang-format` failed on line 3474 with exit code 1
error: recipe `_setup-common` failed with exit code 1
error: recipe `setup` failed with exit code 1
```

`setup-clang-format` runs inside `_setup-common`, which the Zephyr setup step invokes
before it touches a Zephyr workspace. So the lane never reaches Zephyr at all: the
container image these jobs run in has no `unzip`, and the clang-format prebuilt is a
zip.

## Why it matters, and what it cost

The whole Zephyr half of the nightly — 3.7 and 4.4, C, C++ and Rust, talker, listener,
service client/server, action client/server — reports `failure` every night for a
missing system package. Nothing about the code under test is exercised. Worse, the
RTOS jobs downstream of it do not run either: in this same run `bootstrap-probe` and
`installed-probe` are `skipped`, and no `threadx_*` / `nuttx` / `freertos` / `esp32`
job appears at all.

**This was misattributed for several days, and the step name is why.** The MAIN-HEALTH
runbook lists "nightly `zephyr *` jobs failing at `Set up Zephyr 3.7/4.4 workspace`" as
the provisioning class of issue 1158, with an instruction to verify the step name still
matches before attributing. The step name matched every time; the error underneath it
did not. That is the uniformly-red-lane hazard exactly — the lane had no signal
capacity, so a second, different cause read as the first one.

## What this is NOT

- **Not issue 1158.** That is tier 2 / `run-matrix` not reaching its cells, and the
  stage axis `matrix-triage` reports. This is `_setup-common` failing before any
  Zephyr work begins, in a different workflow.
- **Not issue 1353.** No `No space left on device`, no truncated log; this job fails
  in seconds with a complete diagnostic that names the missing package.
- **Not a nano-ros regression.** `nros setup` behaves correctly: it detects the missing
  prereq, names it, prints the exact `apt-get install` line, and refuses rather than
  half-installing. Issue 0368 is the archived case that made it behave that way.

## What would close it

The provisioner already prints the remedy it wants; the question is only where that
belongs, and that is a decision rather than a guess:

1. **The container image** — add `unzip` to the image these nightly jobs run in. Right
   if the image is meant to satisfy `_setup-common`'s declared prereqs.
2. **The workflow** — an apt step before `just setup`, the way other lanes provision
   their own prereqs. Right if the image is meant to stay minimal.
3. **The provisioner** — ship the clang-format dist as a tarball, or unpack the zip
   with something already present. Right if "a prebuilt tool must not need a system
   package to unpack" is the rule we want; that is the widest fix and the only one
   that also helps a user on a bare host.

Acceptance is the `zephyr 3.7 / rust/talker` nightly job reaching a Zephyr build —
green or red on its own cell — and the RTOS jobs downstream of it running at all.
