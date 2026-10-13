---
id: 1870
title: "The Zephyr 4.4 venv had no `anytree`, so `west build -t ram_report` / `-t rom_report` failed on every build"
status: resolved
type: bug
area: [zephyr, build]
severity: low
found: 2026-10-13
resolved: 2026-10-13
related: [0078, 0878, 1758]
---

## Measured

On a host whose `~/.nros/workspaces/zephyr/4.4/.venv312` was set up to this
repo's package list, every size report failed while `west build` succeeded:

```
ModuleNotFoundError: No module named 'anytree'
```

Found by simple-autoware-safety-island (U2), whose board build runs
`west build -t ram_report` / `-t rom_report` to account for DTCM and SRAM.

## Cause

Zephyr's `scripts/footprint/size_report` imports `anytree`;
`zephyr/scripts/requirements-base.txt` lists it under "for ram/rom reports".
`scripts/check-python-deps.py`'s `zephyr-build` group is deliberately a subset
of that file (what `west build` imports), and it is also the list
`provision-py312-venv.sh --create` installs and the remedy text prints. So a
4.4 venv built from it never had `anytree`, and nothing said so.

## Fix

- New group `zephyr-size-report` (`anytree`) in `check-python-deps.py`, kept
  separate from `zephyr-build` by that group's own rule: no nano-ros lane runs
  a size report, so its absence must not fail `just zephyr setup` (nor require
  a CI image change through `check-ci-image-python-deps`).
- `provision-py312-venv.sh --create` installs it with the other two groups, and
  its remedy text lists `anytree`.
- `provision-py312-venv.sh`, `scripts/zephyr/setup.sh` and `just zephyr
  doctor` WARN with the install line when the Zephyr interpreter lacks it.

## Evidence

- `provision-py312-venv.sh <scratch-ws> --create`: venv created by uv
  (CPython 3.12.11), `import anytree` -> 2.12.1.
- Against the existing host venv (no `anytree`): the new WARN line with the
  `uv pip install --python ... anytree` remedy, exit 0.
- After installing `anytree` into that venv, `west build -d <island
  build-board> -t ram_report` rc=0 and printed the full RAM tree (total
  393580).
