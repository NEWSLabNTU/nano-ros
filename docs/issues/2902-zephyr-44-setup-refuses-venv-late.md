---
id: 2902
title: "`just zephyr setup` on the 4.4 line refuses a missing Python 3.12 venv only after `west update`, and its remedy omits the opt-in that would create it"
status: open
type: bug
area: [zephyr, tooling]
severity: low
found: 2026-11-23
related: [issue-0093, issue-1870, issue-2821]
---

## What happens

On a host with no `<ws>/.venv312`, `NROS_ZEPHYR_VERSION=4.4 just zephyr setup`
runs `west init` and the whole `west update` first, then stops at the patch
step:

    ERROR: the Zephyr 4.4 line needs a Python >=3.12 venv at
    /home/aeon/.nros/workspaces/zephyr/4.4/.venv312 — none found.
    ...
    zephyr setup: the 4.4 patch set failed, so .../zephyr/4.4 is not (fully) patched;
      a build there fails at configure.
    error: recipe `setup` failed with exit code 1

Measured on host 142, 2026-11-23: rc=1 after 366 s, and the workspace was left
fetched but unpatched. The same refusal had already stopped the 4.4 setup on
host 241, so two sessions paid the fetch before they learned of a precondition
the script could have checked first.

## Why

The venv check lives in `scripts/zephyr/provision-py312-venv.sh`, and
`scripts/zephyr/patches/4.4.sh` calls it. `scripts/zephyr/setup.sh` applies the
patch set only after `west init -l` and `west update`. So setup checks this
precondition last, even though it needs nothing from the fetch.

The refusal text is otherwise good: it names the path and the exact
`uv venv --python 3.12` / `pip install …` commands. It does not mention
`NROS_ZEPHYR_CREATE_VENV=1`, which `patches/4.4.sh` already honours and which
passes `--create` so the venv is made for you. That opt-in is documented
nowhere else. `git grep NROS_ZEPHYR_CREATE_VENV` finds only `patches/4.4.sh`.

## What is NOT the defect

- **Not creating the venv by default is deliberate.** See the header of
  `provision-py312-venv.sh`: PEP 668, `--user` vs venv vs pipx, and distro
  package names make this a host decision. Issue 0885 applies the same rule to
  `just dev-tools`. The opt-in stays opt-in.
- **Builds already resolve west from the venv.** Issue 2821 taught
  `nros_zephyr_python` the line's Python floor, so the 4.4 build roads pick
  `.venv312` without a hand PATH edit. The "prepend … to PATH" sentence in the
  refusal text predates 2821 and now overstates what a user must do.

## Direction

1. In `setup.sh`, check the line's interpreter precondition BEFORE
   `west init` / `west update`: run `provision-py312-venv.sh "$WS"` in check
   mode, or pass `--create` when the opt-in is set. Refuse up front with the
   same remedy text.
2. Add `NROS_ZEPHYR_CREATE_VENV=1 just zephyr setup` to that remedy text as the
   one-step option, and document it where `just zephyr setup` is documented.
3. Drop or soften the PATH sentence. Point at 2821's resolver instead.

Acceptance: on a host without `.venv312`, `NROS_ZEPHYR_VERSION=4.4 just zephyr
setup` refuses in seconds with no fetch, names both remedies, and with the
opt-in set it provisions the venv and completes.
