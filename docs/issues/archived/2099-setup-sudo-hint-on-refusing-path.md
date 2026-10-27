---
id: 2099
title: "`nros setup` printed \"(or re-run with --sudo)\" on paths that refuse `--sudo`"
status: resolved
type: bug
area: [setup, cli, nuttx, zephyr]
severity: low
found: 2026-10-27
resolved: 2026-10-27
related: [issue-2068, issue-1038, issue-1935, RFC-0062, RFC-0099]
---

## What

A plan's system-package ask ended with one fixed line, built in
`SystemAsk::remedy`:

```text
Install with (or re-run with --sudo):
  sudo apt-get install -y python3-venv unzip || { … }
```

That text was printed by every path that renders an ask, but `--sudo` was
accepted by only two of them (`--system`, `--tool <name>`). Since issue 2068
gave `[source.*]` a `system = [..]` list, a NuttX board's plan asks for
`python3-venv` and `unzip`, so `nros setup qemu-armv7a-nuttx` printed the hint
— and `nros setup qemu-armv7a-nuttx --sudo` was refused with "`--sudo` executes
an install and needs to know WHAT to install". The Zephyr board path printed it
the same way for `[tool.zephyr-sdk] system = ["wget"]`. `--system --check`
printed it too, while clap rejects `--check --sudo` together.

The class had a second half: the `--sudo` check ran AFTER the `--list` and
`--licenses` arms returned, so `nros setup --list --sudo` exited 0 with the flag
silently dropped (issue 1935's shape).

The book worked around it: `integration-nuttx.md` carried a manual
`sudo apt-get install -y python3-venv unzip` block (`probe=35`) after
`nros setup`, because the command that composed that line could not run it.

## Fix

- `nros setup <board> --sudo` and `--workspace --sudo` install the board plan's
  one system ask before any download, then resolve the plan again over what the
  probes answer after it. `--tool --sudo`, `--system --sudo` and the board path
  all go through one installer (`install_system_ask`; the board and tool paths
  through `resolve_installing_system`), replacing two inline copies of the
  `sh -c` install.
- The hint is a parameter: each caller passes the `nros setup … --sudo` line for
  its OWN selectors (`sudo_rerun_line`), or none — `--system --check` offers
  `nros setup --system [--role …] --sudo`, and a path already running under
  `--sudo` offers nothing.
- `--sudo` is validated in `route()`, before any arm can return, by
  `sudo_refusal`: an arm with no OS install refuses it and lists the four lines
  that take it.
- `integration-nuttx.md` drops the manual apt block; both `probe=30` setup
  lines carry `--sudo` (the probe container is root with a `sudo` shim, so the
  composed `sudo apt-get …` runs there as on a user's host).

## Evidence

Unit tests in `packages/cli/nros-cli-core/src/cmd/setup.rs`, executor injected
so no test runs `sudo`:

- `shipped_index_nuttx_board_sudo_installs_the_plans_ask_once` — real index,
  both NuttX boards: `--sudo` runs exactly one `sudo apt-get install -y …`
  naming both packages and the re-resolved ask is empty; without `--sudo`, and
  under `--dry-run`, nothing runs and the ask stands.
- `sudo_on_an_arm_with_nothing_to_install_is_refused_with_lines_that_take_it`
  — every line the refusal offers is one `route()` accepts.
- `every_sudo_rerun_line_an_ask_offers_is_accepted`,
  `a_printed_ask_offers_the_callers_own_sudo_line_or_none`,
  `install_system_ask_without_a_manager_refuses_and_runs_nothing`.

Mutation-checked: dropping the install from `resolve_installing_system`,
accepting `--sudo` on a board-less or `--list` route, removing the route check,
and restoring the old fixed hint each turn one of these red.
