---
id: 1070
title: "`--sudo` required `--system` in clap, so issue 1038's `--tool <name> --sudo` was unreachable and the nightly esp32 lane never built"
status: resolved
area: cli
severity: high
related: [1038, 1025, 0062]
---

## What

`nros setup`'s `--sudo` flag carried `requires = "system"`:

```rust
/// issue 1038 — ALSO applies to `--tool`: a tool declaring its own
/// `[tool.<name>] system = [..]` build deps installs them instead of
/// bailing with a line for a human to copy.
#[arg(long, requires = "system", conflicts_with = "check")]
pub sudo: bool,
```

The doc comment and the attribute contradict each other. The install path for
the `--tool` form exists and is correct (`setup.rs`, the `run_sudo` branch that
prints `nros setup --tool {name} --sudo: running:`), and **clap rejected every
invocation that could reach it**. `nros setup --tool esp32-qemu --sudo` exited 2
on `the following required arguments were not provided: --system`.

So issue 1038 shipped a feature nothing could call.

## How it surfaced

`nightly.yml` runs exactly that line:

```yaml
- name: Provision esp32-qemu's declared system deps (issue 1038)
  run: nros setup --tool esp32-qemu --sudo
```

It exited 2, and because it is a plain step, **every step after it was skipped —
including `Build (esp32)`**. The esp32 cell was red in every nightly.

It stayed invisible because a DIFFERENT failure was in front of it. While issue
1025 was live, `Build (esp32)` ran and failed on the flash packer, so that is
what the log showed and what the workflow comment blamed:

> NOT the cause of this cell's RED — that is issue 1025 (#303), where the flash
> packer looks in a directory the build stopped using.

That comment was correct about 1025 and wrong to acquit this step. With #303
merged, a dispatched nightly on `main` moved the failure from `Build (esp32)` to
this step, with everything after it skipped — which is what named it.

Two independent faults in one lane, the second only observable once the first
was gone. A red lane reports its FIRST failure, not its faults.

## Fix

`--sudo` no longer requires `--system`. It is validated in `run()` instead,
which can say which of the two forms is missing rather than naming one of them:

```
`--sudo` executes an install and needs to know WHAT to install.
  --system --sudo         the `[system.*]` OS-package closure
  --tool <name> --sudo    that tool's own `[tool.<name>] system = [..]` build deps
```

`conflicts_with = "check"` stays — one reports, the other acts.

Regression test: `sudo_parses_with_tool_as_well_as_system`, at PARSE level,
because the defect was parse-level. The install code underneath was correct and
unreachable, so no test of that code could have failed. Verified in both
directions — restoring `requires = "system"` fails the test with
`MissingRequiredArgument`.

## Why no test caught it before

The feature's acceptance was the install BEHAVIOUR, and that behaviour was
correct. Nothing asserted the CLI could be invoked in the shape the feature
documented. A flag's reachability is part of its contract, and the only place
that shows is argument parsing.
