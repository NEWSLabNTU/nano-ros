---
id: 1296
title: "`nros build` in a single-package C/C++ leaf fails — the synthesised bringup is named after the DIRECTORY, the cmake driver asks for `[system] name`"
status: open
type: bug
area: tooling, cli, examples
severity: medium
found: 2026-09-11
related: [rfc-0098, phase-445]
---

# What happens

In a single-package C leaf converted by phase-445 W3b, `nros sync` succeeds and
`nros build` then fails at cmake configure:

```console
$ cd examples/native/c/talker
$ nros sync
sync: resolved system.launch.xml → build/nros/models/talker/system_model.yaml
sync: done.
$ nros build
nros build: native_c_talker:native -> board native (platform posix), driver cmake
CMake Error at cmake/nano_ros_workspace_metadata.cmake:68 (message):
  nano_ros_workspace_metadata: `nros plan` failed (rc=1)
  stderr: Error: no SystemModel for `native_c_talker/launch/system.launch.xml`.
    It is a BUILD ARTIFACT (phase-330 W4), so generate it rather than
    committing one:
      nros sync            # writes <ws>/build/nros/models/<bringup>/
```

The remedy the message names is the command that was just run, so the error is
also a dead end for the reader.

# Why

The leaf's synthetic bringup identity is derived TWICE, from two different
facts, and they disagree:

- `ws.rs`'s `synthesise_leaf_launch` names it after the leaf's **directory**:
  `<leaf>/build/nros/launch/<leaf-dir>/system.launch.xml` — here `talker`.
- `cmake/nano_ros_workspace_metadata.cmake` asks `nros plan` for
  `${_NRW_SYSTEM}`, the **`[system] name`** — here `native_c_talker` — and
  builds `${_bringup_dir}/launch/system.launch.xml` from it.

For `examples/native/c/talker` those are `talker` and `native_c_talker`. The
two agree only where a leaf's directory name happens to equal its `[system]
name`, which is true of no converted leaf I checked.

The same disagreement exists for a Rust leaf (`examples/native/rust/talker`
resolves to `build/nros/models/talker/` while `[system] name` is
`native_talker`) and is simply never asked: `nros build` takes the cargo driver
there, which reads the model through `model_location` rather than through
`nros plan`.

# Consequence

`nros build` is usable in a single-package **Rust** leaf and not in a
single-package **C or C++** one. The C/C++ leaf path that does work is the
leaf's own CMake, which does not go through `nano_ros_workspace` at all:

```console
$ cd examples/native/c/talker
$ cmake -B build && cmake --build build      # measured green, with and without a prior `nros sync`
```

So this blocks phase-445's acceptance sentence ("a fresh clone, then
`nros build <image>`, builds one image per board family") for the C and C++
families only, and it is why `book/src/getting-started/first-node-c.md` and
`first-node-cpp.md` still document the leaf `cmake` pair rather than
`nros build` (phase-445 W7).

# Fix

One derivation, as usual. Either:

- the synthesiser names the bringup dir from the `[system] name` it already
  read (`decl`), so both sides answer the same question; or
- `nano_ros_workspace_metadata` receives the bringup directory rather than
  recomputing it from `_NRW_SYSTEM`.

The first is preferable — `[system] name` is the authored fact, the directory
name is an accident of where the user put the package — but it moves a path
that other readers may key on, so the change wants a sweep over every consumer
of `build/nros/launch/<x>/` and `build/nros/models/<x>/`, plus an acceptance
BUILD of a C leaf, a C++ leaf and a Rust leaf through `nros build`.

# Measured

2026-09-11, worktree at `efd1c2c57` + phase-445 W4b/W5 cherry-picks:

- `examples/native/c/talker`: `nros sync` OK, `nros build` fails as above;
  `cmake -B build && cmake --build build` produces `build/c_talker` (10.6 MB).
- A copy of the same leaf outside the checkout, with no `nros sync` at all,
  configures and builds identically — the C leaf's message bindings are a
  CMake-time output, so sync is not a precondition there.
- `examples/native/rust/talker`: `nros sync` + `nros build` both OK,
  `build/native/target/debug/talker`.
