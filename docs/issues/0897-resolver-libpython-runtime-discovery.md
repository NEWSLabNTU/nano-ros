---
id: 897
title: "`nros-launch-resolve` hard-links one `libpython` soname, so one build serves
  one interpreter — and abi3, which issue 0400 recommends, does not apply to embedding"
status: open
type: tech-debt
area: tooling
related: [issue-0400, issue-0285, issue-0409, rfc-0060]
---

## Problem

Python launch files are part of the ROS 2 standard and nano-ros supports them on
purpose, so `nros-launch-resolve` embeds CPython through pyo3 and the link is
not optional. What *is* a choice is HOW it links:

```
$ readelf -d nros-launch-resolve | grep python
  (NEEDED)  Shared library: [libpython3.10.so.1.0]
$ nm -D nros-launch-resolve | grep -c ' U dlopen'
  0
```

A hard `DT_NEEDED` on a **minor-version soname**, with no `dlopen`. One build
therefore serves exactly one interpreter, and the loader fails before `main`, so
the binary cannot report its own failure:

```
lr-broken: error while loading shared libraries: libpython9.99.so.1.0:
           cannot open shared object file
rc=127
```

This is the same coupling [issue 0400](archived/0400-box-host-share-target-dirs.md)
hit across a distrobox boundary: a host build links `libpython3.14.so` (Arch),
the box cannot load it; a box build links `3.10`, the host cannot.

**The crash is already handled** — `nros sync` captures the child's stderr, and
since `c0215ec2c` it converts a loader failure into a diagnostic naming the
missing library, this host's `python3`, both remedies, and the fact that
XML/YAML launch files need no interpreter. That closes the *legibility* half.
This issue is the other half: one artifact, many interpreters.

## Issue 0400's recommendation does not work, and it is archived

0400 says:

> Prefer **abi3**: build the resolver's pyo3 against the CPython *stable ABI* so
> ONE binary loads whatever `libpython3` the running side provides […] adding
> the `abi3-py3N` feature […] makes the compiled binary limited-API and
> version-agnostic across CPython ≥ floor

and lists as an unresolved caveat "confirm neither pyo3's embedding path nor the
launch parser's Python usage needs a non-limited symbol".

The caveat is moot, because the premise is wrong. Checked against PyO3's own
documentation (2026-08-29):

- the building-and-distribution guide has **no abi3-in-embedding section**;
  `abi3` is presented throughout as an *extension module* mechanism;
- for an embedded interpreter the ABI is fixed by **the `libpython` linked at
  build time**, not by `Py_LIMITED_API`;
- there is **no supported link against a version-agnostic `libpython3.so`**
  stub — `PYO3_PYTHON` selects a concrete interpreter, and `PYO3_NO_PYTHON`
  exists for building stable-ABI *modules* with no interpreter present;
- upstream guidance for "one artifact across CPython versions with an embedded
  interpreter" is to **stop embedding and become a cdylib extension module**.

So an archived issue currently points the next reader at a dead end. That is
worth more than the technical detail: 0400 is `resolved`, its recommendation
reads as settled, and nobody would re-check it.

## What actually would work

1. **`dlopen` the host's `libpython` at runtime.** Drop the `DT_NEEDED`,
   discover the interpreter (`python3 -c 'import sysconfig'` gives `LIBDIR` and
   `INSTSONAME`), `dlopen` it, bind the symbols pyo3 needs. One binary, any
   CPython ≥ floor, and a *catchable* failure instead of a loader abort. This is
   the direction, and it is upstream work in `play_launch` (the pyo3 dep lives
   in the submodule), not a nano-ros edit.
2. **Invert the boundary** — build the parser as an abi3 cdylib and drive it
   from a small Python entry point. Then abi3 genuinely applies. Cost: `nros`
   gains a runtime dependency on a host `python3` it does not control, and the
   parser's Rust-implemented `launch`/`launch_ros` mocks (which it injects into
   `sys.modules`, blocking the real ROS packages via a `sys.meta_path` hook)
   must all fit the limited API. That last point is unverified and is the thing
   to check first — if the `#[pyclass]` mocks cannot be limited-API, this option
   collapses.
3. **Build per-interpreter and select at runtime.** Honest, unglamorous, and
   multiplies build cost by the number of supported interpreters.

Option 1 is preferred: it removes the coupling without changing who owns the
process, and it is the only one that turns the failure into something a program
can handle rather than something a loader kills.

## Not doing

- **Dropping `.launch.py` support.** It is in the ROS 2 standard and
  compatibility is the point of the project.
- **Vendoring a CPython.** Solves the version problem by pinning it, which is
  what we already do.

## Acceptance

- One `nros-launch-resolve` build runs against at least two CPython minor
  versions on the same machine.
- A host with no Python at all gets the `c0215ec2c` diagnostic, not a loader
  abort — already true, and must stay true.
- `docs/issues/archived/0400-*.md` carries a correction pointing here, so its
  abi3 recommendation stops reading as settled advice.
