---
id: 2660
title: "The launch resolver rebuilt pyo3-ffi and everything above it whenever PATH changed"
status: resolved
type: perf
severity: low
area: [build, cli]
related: [phase-478]
found: 2026-11-17
resolved: 2026-11-17
resolved_in: "phase-478 I25"
---

GitHub: NEWSLabNTU/nano-ros#4084

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
I25** (island and consumer findings). Found by the Autoware safety island,
which puts the `nros` CLI on PATH before it calls the nano-ros recipes.

## What happened

`just setup-launch-resolve` rebuilt the resolver whenever PATH differed from
the previous build, by any entry:

    Dirty pyo3-ffi v0.29.2: the env variable PATH changed

About 30 s on CI, and every call from a consumer whose PATH carries its own
`nros` directory.

pyo3-build-config's `find_interpreter` uses `PYO3_PYTHON` when it is set, else
the `VIRTUAL_ENV` / `CONDA_PREFIX` interpreter, else it searches `python` then
`python3` on PATH. Only that last branch prints
`cargo:rerun-if-env-changed=PATH`, and it is the branch every recipe took: none
set `PYO3_PYTHON`. So PATH itself, not the interpreter, was the rebuild key.

## Fix

`scripts/build/pyo3-python.sh` (`nros_pin_pyo3_python`) exports `PYO3_PYTHON`
as the absolute path of the interpreter pyo3 would have found, in pyo3's own
order. The build is unchanged; the rebuild trigger moves from PATH to the
interpreter. It leaves an explicit `PYO3_PYTHON` and a venv / conda
environment alone (pyo3 does not watch PATH there either), and never exports
an empty value, which pyo3 would take as an interpreter named "".

Every recipe that builds a pyo3 crate sources it:

- `just setup-launch-resolve` (the resolver and its Python half). This is also
  the CI step: `host-tests.yml` and `.github/actions/setup-nros-cli` build the
  resolver through this recipe.
- `scripts/bootstrap.sh`'s resolver build, so it and the recipe agree.
- The CLI workspace lanes that build `colcon-nano-ros` (pyo3 0.29):
  `check cli-clippy`, `check cli-tests`, `test-doc`, `test-ignored`.

Not pinned: `release-nros.yml` builds the resolver once in a fresh job, so no
PATH change can reach a second build; `just setup-cli` builds `--bin nros`,
which has no pyo3 in its graph.

## Gate

`just check pyo3-path-stable` builds pyo3-ffi from the resolver's
lock in a scratch target. Unpinned, a prepended PATH entry must report
`Dirty pyo3-ffi ...: the env variable PATH changed` (the control); pinned, it
must report `Fresh pyo3-ffi`.
