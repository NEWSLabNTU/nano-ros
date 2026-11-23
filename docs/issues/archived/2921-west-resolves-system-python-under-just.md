---
id: 2921
title: "Under `just`, a Zephyr 4.4 fixture build runs the system `west` and configures with Python 3.10, even with the line's venv first on PATH"
status: resolved
type: bug
area: [zephyr, build, tooling]
severity: medium
found: 2026-11-24
resolved: 2026-11-24
related: [issue-0698, issue-2821, issue-2902]
---

## What happens

On host 241, `NROS_ZEPHYR_VERSION=4.4 NROS_ZEPHYR_FIXTURE_FILTER=… just zephyr
build-fixtures` for `build-ws-rust-talker-clock-sntp-zenoh` failed at
configure:

    Could NOT find Python3: Found unsuitable version "3.10.12", but required is
    at least "3.12" (found /usr/bin/python3, found components: Interpreter)
    FATAL ERROR: command exited with status 1: /usr/bin/cmake -DWEST_PYTHON=/usr/bin/python3 …

The caller had `~/.nros/workspaces/zephyr/4.4/.venv312/bin` first on PATH, and
issue 2821's resolver (`nros_zephyr_python`) answered that venv for the line.
Reproduced on 2026-11-24 in a fresh build directory, so this is not a stale
cache.

## Why

`-DWEST_PYTHON` is the interpreter running `west`, so the `west` that ran was
`~/.local/bin/west`, on the system Python.

1. The root `justfile` exports
   `PATH := <checkout>/packages/cli/target/release:~/.local/bin:$PATH`, so
   inside every recipe `~/.local/bin` comes before the caller's venv.
2. `nros_zephyr_activate` (`scripts/build/zephyr-python.sh`) says it puts the
   interpreter's bin directory FIRST on PATH. It actually only prepended it
   when the directory was absent. The venv was present, further down, so PATH
   was left alone.

Running `zephyr-fixture-run-one.sh` by hand, outside `just`, with the same
venv first on PATH, built the image (rc 0). That isolates the cause to the
recipe's PATH.

## Resolution

Fixed 2026-11-24. `nros_zephyr_activate` removes the bin directory from
wherever it sits on PATH and puts it first, as its contract says. Every lane
that calls it gets this: the fixture builder, the make-driver leaves, the FVP
recipes and the copy-out check.

A sweep for the same presence-only guard found four more: `activate.sh`
(cargo bin), `scripts/bootstrap.sh` (two) and `scripts/install.sh`. All four
only need the directory to be reachable, so presence is the right test there
and they are unchanged.

Measured on host 241: with PATH `/a:/b:<venv>/bin:/usr/bin`, the function
leaves `<venv>/bin` first and `west` resolves into the venv. Under `just`,
with no PATH edit by the caller, `NROS_ZEPHYR_VERSION=4.4 just zephyr
build-fixtures` built `build-ws-rust-talker-clock-sntp-resync-zenoh` in a fresh
directory ("Zephyr test fixtures built successfully").
