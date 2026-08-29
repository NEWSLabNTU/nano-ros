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

**The design below supersedes this list**, and is a hybrid rather than any one
of them. Option 1's virtue is that we keep owning the process; option 2's is
that abi3 becomes usable. Those are not in tension once the Python half is a
cdylib *we* load — we dlopen libpython ourselves, then load an abi3 extension
object against it, which is what a CPython process does for its own extension
modules. Option 1 alone cannot get abi3 (it is still an embedded binary);
option 2 alone gives up process ownership to a Python entry point. Option 3
stays the fallback if the limited-API check in the open questions fails.


## Design (2026-08-29)

Two requirements, and they decide the shape between them:

1. **No Python on the host ⇒ the resolver still runs.** It scans and resolves
   XML/YAML launch files normally, and fails only when it actually reaches a
   `.launch.py` — naming that file.
2. **Python present ⇒ we choose which one**, and one build works across
   versions.

### Requirement 1 is a LINKAGE problem, not a restructuring one

The parser already dispatches by extension, and the Python boundary is a single
call site (`play_launch_parser/src/lib.rs`):

```rust
match ext {
    "py"           => return self.execute_python_file(path, …),
    "yaml" | "yml" => { self.process_yaml_launch_file(path)?; return Ok(()); }
    _ => {}
}
// … XML below, roxmltree only
```

`src/xml/` contains no pyo3 reference. So the XML/YAML path is *already* Python-
free at the source level; what stops it running is that `libpython` is a
`DT_NEEDED` on the binary, so the loader refuses the process before `main`. Fix
the linkage and requirement 1 falls out — no dispatch rework.

### Shape: a thin driver plus a loadable Python half

```
nros-launch-resolve            pure Rust, NO Python symbols, no DT_NEEDED
  ├── scan / XML / YAML        works with no interpreter at all
  └── first `.launch.py`:
        1. discover interpreter          (below)
        2. dlopen(libpython, RTLD_NOW|RTLD_GLOBAL)
        3. dlopen(libnros_launch_py.so, RTLD_NOW)
        4. call its exported entry point

libnros_launch_py.so           the pyo3 parser, built as an ABI3 CDYLIB,
                               libpython NOT linked — symbols left undefined
```

Step 2 before step 3 is the whole trick, and it is the documented one:
`RTLD_GLOBAL` makes libpython's symbols available for resolution of
*subsequently* loaded objects, which is exactly how a normal CPython process
satisfies an extension module's undefined symbols. We are doing by hand what the
interpreter does for itself.

**This is also where abi3 finally applies.** abi3 is an extension-module
mechanism — useless for the embedded *binary* we have today (see the correction
on issue 0400), and correct for a cdylib. Building the Python half as
`abi3-py3N` makes one `.so` valid for every CPython ≥ floor, so version
selection becomes a runtime choice rather than a build-time pin.

### Why two artifacts rather than one binary with undefined symbols

A single binary could work: link it `-Wl,--unresolved-symbols=ignore-all` and
dlopen libpython before first use. Rejected — that flag tolerates *every*
undefined symbol, so a genuine link error in unrelated code stops being a build
failure and becomes a runtime crash. The split confines "symbols resolved later"
to the one object that needs it, and the driver keeps a normal, strict link.

### Interpreter selection

Discovery is a query, not a guess — ask the interpreter where its library is:

```
python3 -c "import sysconfig; print(sysconfig.get_config_var('INSTSONAME'),
                                    sysconfig.get_config_var('LIBDIR'),
                                    sysconfig.get_config_var('Py_ENABLE_SHARED'))"
# libpython3.10.so.1.0 /usr/lib/x86_64-linux-gnu 1
```

Order, mirroring `scripts/build/zephyr-python.sh` so the tree has ONE answer to
"which interpreter":

1. `$NROS_PYTHON` — the existing repo-wide knob. Explicit wins, usable or not.
2. `python3` on `PATH`.
3. Nothing else. **No scanning for `libpython*.so` on the filesystem** — that is
   the same class as the `$PATH` lookup issue 0285 removed: it finds *an*
   answer, not the *right* one.

The `.so` itself is located beside the driver via `current_exe()`, never by
`$PATH` or `LD_LIBRARY_PATH`, for the same reason.

### The cases that must produce a message, not a crash

| host state | XML/YAML | `.launch.py` |
| --- | --- | --- |
| no `python3` at all | works | error: names the file, says Python launch files need an interpreter |
| `python3` present, no shared libpython (`Py_ENABLE_SHARED=0`) | works | error: names the interpreter and that it was built without `--enable-shared` |
| `python3` older than the abi3 floor | works | error: names both versions |
| `python3` ≥ floor, shared lib present | works | works |

The third and fourth rows are the point of the change. Today every row above the
last is a loader abort with no message from us; since `c0215ec2c` it is at least
a diagnostic, but XML/YAML still cannot run.

### Open questions, in the order that would kill the design

1. **Do the parser's `#[pyclass]` mocks fit the limited API?** The parser
   reimplements `launch` / `launch_ros` / `launch_xml` in Rust and injects them
   into `sys.modules` behind a `sys.meta_path` blocker. That is a large pyclass
   surface, and abi3 restricts pyclass features. **Check this first** — if the
   mocks cannot be limited-API, the version-agnostic half collapses and only the
   graceful-degradation half survives (still worth having, and still requires
   the split).
2. **Initialisation under abi3.** Nothing has initialised the interpreter, so
   the cdylib must call it explicitly rather than relying on pyo3's
   `auto-initialize` (which is documented as not for extension modules). Confirm
   the init entry pyo3 uses is in the stable ABI at the chosen floor.
3. **Floor choice.** Humble ships Python 3.10, Jazzy 3.12. A lower floor works
   on more hosts and permits less C API; pick the lowest the parser compiles
   against.
4. **Performance.** The parser carries deliberate optimisation work (dashmap,
   LRU caches). The limited API is slower on some paths; measure before and
   after on a real bringup rather than assuming it is free.
5. **Where the `.so` ships.** It becomes a second artifact that
   `just setup-launch-resolve` must produce and that every consumer must find.
   That is new surface for the 0285 class of bug, and needs the same absolute-
   path discipline.

All of this is upstream work in the `play_launch` submodule, where the pyo3
dependency lives — not a nano-ros-only edit.

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
